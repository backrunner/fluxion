import { DEFAULTS, HOST, TTL, settingsFrom, selectHeaders, responseSize, responseFilename, matchRequest, captureDecision, handoff, canRecoverCookies } from "./policy.mjs";

// Request credentials live only in the service worker's bounded RAM cache.
// A worker restart loses correlation and safely leaves downloads in Chrome.
const records = new Map();
const inFlight = new Set();
const handled = new Set();
let settings = { ...DEFAULTS };
let lastStatus = "";
const ready = chrome.storage.local.get("settings").then(result => {
  try { settings = settingsFrom(result.settings); } catch { settings = { ...DEFAULTS }; }
});

function prune(now = Date.now()) {
  for (const [id, record] of records) if (now - record.time > TTL) records.delete(id);
  while (records.size > 256) records.delete(records.keys().next().value);
  while (handled.size > 1024) handled.delete(handled.values().next().value);
}
chrome.alarms.create("expire-credentials", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(() => prune());

chrome.webRequest.onSendHeaders.addListener(details => {
  prune();
  if (details.incognito) return;
  let headers;
  try { headers = selectHeaders(details.requestHeaders, settings.extraHeaders); }
  catch { records.delete(details.requestId); return; }
  // A redirected request replaces the previous hop, including its headers.
  records.set(details.requestId, { url: details.url, method: details.method, headers,
    authRequired: records.get(details.requestId)?.authRequired === true,
    headersVisible: (details.requestHeaders?.length || 0) > 0,
    type: details.type, initiator: details.initiator, tabId: details.tabId, frameId: details.frameId,
    time: details.timeStamp, incognito: details.incognito, status: 0, size: null });
  prune();
}, { urls: ["http://*/*", "https://*/*"], types: ["main_frame", "sub_frame", "xmlhttprequest", "other"] }, ["requestHeaders", "extraHeaders"]);

chrome.webRequest.onAuthRequired.addListener(details => {
  const record = records.get(details.requestId);
  if (record) record.authRequired = true;
}, { urls: ["http://*/*", "https://*/*"] });

chrome.webRequest.onHeadersReceived.addListener(details => {
  const record = records.get(details.requestId);
  if (!record || record.url !== details.url) return;
  record.status = details.statusCode;
  record.responseTime = details.timeStamp;
  record.size = responseSize(details.responseHeaders);
  record.filename = responseFilename(details.responseHeaders);
}, { urls: ["http://*/*", "https://*/*"] }, ["responseHeaders"]);
chrome.webRequest.onErrorOccurred.addListener(details => {
  // Chrome may report ERR_ABORTED while moving a navigation into downloads.
  if (details.error !== "net::ERR_ABORTED") records.delete(details.requestId);
}, { urls: ["http://*/*", "https://*/*"] });

const api = {
  send: message => chrome.runtime.sendNativeMessage(HOST, message),
  pause: id => chrome.downloads.pause(id),
  resume: id => chrome.downloads.resume(id),
  cancel: id => chrome.downloads.cancel(id),
  get: async id => (await chrome.downloads.search({ id }))[0],
};

async function downloadHeaders(item, record) {
  if (record.headersVisible) return record.headers;
  if (!canRecoverCookies(item, record) || settings.extraHeaders.length) throw new Error("unavailable_headers");
  const stores = await chrome.cookies.getAllCookieStores();
  const storeId = stores.find(store => store.tabIds.includes(record.tabId))?.id;
  if (!storeId) throw new Error("unknown_cookie_store");
  const url = item.finalUrl || item.url;
  const { partitionKey } = await chrome.cookies.getPartitionKey({ tabId: record.tabId, frameId: record.frameId });
  const unpartitioned = (await chrome.cookies.getAll({ url, storeId })).filter(cookie => !cookie.partitionKey);
  const partitioned = partitionKey ? (await chrome.cookies.getAll({ url, storeId, partitionKey })).filter(cookie => cookie.partitionKey) : [];
  const cookies = [...unpartitioned, ...partitioned].sort((a, b) => b.path.length - a.path.length);
  // Same-name partitioned/unpartitioned cookies can have ambiguous ordering.
  const keys = new Set();
  for (const cookie of cookies) {
    const key = `${cookie.name}\n${cookie.path}`;
    if (keys.has(key)) throw new Error("ambiguous_cookies");
    keys.add(key);
  }
  const headers = [
    { name: "referer", value: item.referrer },
    { name: "user-agent", value: navigator.userAgent },
  ];
  if (cookies.length) headers.push({ name: "cookie", value: cookies.map(c => `${c.name}=${c.value}`).join("; ") });
  return headers;
}

async function report(status) {
  const messages = {
    pending: "请在 Fluxion 的添加任务窗口中确认，或取消以继续浏览器下载。",
    created: "已在 Fluxion 添加任务，可在任务列表中启动。",
    cancelled: "已取消添加，下载继续由浏览器处理。",
    unavailable: "未能连接或创建任务，下载仍由浏览器处理。请打开新版 Fluxion 后重试。",
    browser_kept: "任务已添加到 Fluxion；浏览器下载未取消，请在浏览器中检查。",
    unmatched: "无法准确匹配原请求，已保留浏览器下载。请重新点击下载链接。",
    skip: "此下载暂时无法接管。",
  };
  lastStatus = messages[status] || "";
  const failed = ["unavailable", "browser_kept", "unmatched"].includes(status);
  await chrome.action.setBadgeText({ text: failed ? "!" : status === "pending" ? "…" : "" });
  await chrome.action.setBadgeBackgroundColor({ color: "#9b4b30" });
  await chrome.action.setTitle({ title: lastStatus || "Fluxion" });
}

async function consider(item, manual = false) {
  await ready;
  if (inFlight.has(item.id) || (!manual && handled.has(item.id))) return "skip";
  prune();
  const record = matchRequest(item, records);
  if (record) record.downloadId = item.id;
  const decision = captureDecision(item, settings, record, manual);
  if (decision !== "capture") {
    if (manual) await report(decision);
    return decision;
  }
  inFlight.add(item.id);
  handled.add(item.id);
  try {
    let headers;
    try { headers = await downloadHeaders(item, record); }
    catch { await report("unmatched"); return "unmatched"; }
    await report("pending");
    const status = await handoff(api, item, {
      url: item.finalUrl || item.url,
      filename: item.filename?.split(/[\\/]/).pop() || record.filename || null,
      headers,
    });
    await report(status);
    return status;
  } finally {
    inFlight.delete(item.id);
  }
}

chrome.downloads.onCreated.addListener(item => { void consider(item).catch(() => report("unavailable")); });
chrome.downloads.onChanged.addListener(delta => {
  if (delta.state?.current === "complete" || delta.state?.current === "interrupted") return;
  if (delta.totalBytes || delta.filename) {
    void api.get(delta.id).then(item => item && consider(item)).catch(() => {});
  }
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.settings) {
    try { settings = settingsFrom(changes.settings.newValue); } catch { settings = { ...DEFAULTS }; }
    // Do not accidentally reuse headers captured under the previous policy.
    records.clear();
  }
});

chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (sender.id !== chrome.runtime.id || !sender.url?.startsWith(chrome.runtime.getURL("options.html"))) return false;
  (async () => {
    await ready;
    if (message.action === "status") {
      const items = await chrome.downloads.search({ state: "in_progress", limit: 8, orderBy: ["-startTime"] });
      return { settings, lastStatus, downloads: items.filter(item => !item.incognito).map(item => ({
        id: item.id, filename: item.filename?.split(/[\\/]/).pop() || "下载文件", totalBytes: item.totalBytes,
      })) };
    }
    if (message.action === "save") {
      const next = settingsFrom(message.settings);
      await chrome.storage.local.set({ settings: next });
      return { ok: true };
    }
    if (message.action === "ping") {
      try { const response = await api.send({ version: 1, action: "ping" }); return { ok: response?.ok === true }; }
      catch { return { ok: false }; }
    }
    if (message.action === "capture" && Number.isSafeInteger(message.id)) {
      const item = await api.get(message.id);
      if (item) await consider(item, true);
      return { lastStatus };
    }
    return { ok: false };
  })().then(reply).catch(() => reply({ ok: false, lastStatus: "操作未完成，请重试。" }));
  return true;
});
