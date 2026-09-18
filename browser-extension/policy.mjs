export const DEFAULTS = Object.freeze({ enabled: false, minimumMiB: 10, captureUnknown: false, extraHeaders: [] });
export const HOST = "top.backrunner.fluxion";
export const TTL = 60_000;

export function settingsFrom(value = {}) {
  const minimumMiB = Number(value.minimumMiB ?? DEFAULTS.minimumMiB);
  if (!Number.isFinite(minimumMiB) || minimumMiB < 0 || minimumMiB > 1_048_576) {
    throw new Error("文件大小必须为 0–1048576 MiB。");
  }
  const extraHeaders = [...new Set((value.extraHeaders ?? []).map(s => s.trim().toLowerCase()).filter(Boolean))];
  if (extraHeaders.length > 20 || extraHeaders.some(s => !/^x-[a-z0-9-]{1,125}$/.test(s) || /^x-forwarded-/.test(s))) {
    throw new Error("额外请求头仅支持 X- 开头的名称，最多 20 个，不支持 X-Forwarded-*。");
  }
  return { enabled: value.enabled === true, minimumMiB, captureUnknown: value.captureUnknown === true, extraHeaders };
}

export function httpUrl(value) {
  try { const url = new URL(value); return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password; }
  catch { return false; }
}

export function selectHeaders(headers = [], extraHeaders = []) {
  const allowed = new Set(["cookie", "authorization", "referer", "origin", "user-agent", "accept", "accept-language", ...extraHeaders]);
  const selected = new Map();
  for (const header of headers) {
    const name = header.name.toLowerCase();
    if (allowed.has(name) && typeof header.value === "string") {
      // Preserve split Cookie fields, but reject ambiguous singleton fields.
      if (selected.has(name) && name !== "cookie") throw new Error("ambiguous_headers");
      selected.set(name, selected.has(name) ? `${selected.get(name)}; ${header.value}` : header.value);
    }
  }
  return [...selected].map(([name, value]) => ({ name, value }));
}

export function responseSize(headers = []) {
  const encoding = headers.find(h => h.name.toLowerCase() === "content-encoding")?.value;
  if (encoding && encoding.toLowerCase() !== "identity") return null;
  const length = headers.find(h => h.name.toLowerCase() === "content-length")?.value;
  if (!/^\d+$/.test(length ?? "")) return null;
  const size = Number(length);
  return Number.isSafeInteger(size) ? size : null;
}

export function responseFilename(headers = []) {
  const value = headers.find(h => h.name.toLowerCase() === "content-disposition")?.value || "";
  const extended = /(?:^|;)\s*filename\*=UTF-8'[^']*'([^;]+)/i.exec(value);
  if (extended) { try { return decodeURIComponent(extended[1].trim()); } catch { /* Use ordinary filename. */ } }
  const plain = /(?:^|;)\s*filename\s*=\s*(?:"([^"]+)"|([^;]+))/i.exec(value);
  return plain ? (plain[1] || plain[2]).trim() : null;
}

export function captureDecision(item, settings, record, manual = false) {
  if (!httpUrl(item.finalUrl || item.url) || item.state !== "in_progress" || item.incognito) return "skip";
  if (!manual && (!settings.enabled || item.paused)) return "skip";
  if (record?.method !== "GET" || record.status < 200 || record.status >= 300 || record.status === 206) return "unmatched";
  if (manual) return "capture";
  // Chrome also reports 0 (not just -1) while a chunked size is unknown.
  // An explicit Content-Length: 0 distinguishes a genuinely empty resource.
  const size = Number.isSafeInteger(item.totalBytes) && item.totalBytes > 0 ? item.totalBytes : record.size;
  if (size === null || size === undefined) return settings.captureUnknown ? "capture" : "unknown";
  return size >= settings.minimumMiB * 1024 * 1024 ? "capture" : "small";
}

// downloads has no requestId/tabId. Match only a unique, contemporaneous final
// request, with the browser's actual referrer. Never merge redirect credentials.
export function matchRequest(item, records, now = Date.now()) {
  const start = Date.parse(item.startTime);
  if (!Number.isFinite(start)) return null;
  const finalUrl = item.finalUrl || item.url;
  const matches = [...records.values()].filter(r =>
    (r.downloadId === undefined || r.downloadId === item.id) &&
    r.url === finalUrl && !r.incognito && r.responseTime && now - r.responseTime <= TTL &&
    Math.abs(r.responseTime - start) <= 10_000 &&
    ((r.headers.find(h => h.name === "referer")?.value || "") === (item.referrer || "") || canRecoverCookies(item, r))
  );
  return matches.length === 1 ? matches[0] : null;
}

export function canRecoverCookies(item, record) {
  // Chromium hides all request headers for some <a download> requests.
  // Only recover a same-origin download tied to a known non-incognito frame.
  if (record.headersVisible !== false || record.type !== "other" || record.tabId < 0 || record.frameId !== 0 || record.incognito || record.authRequired) return false;
  try {
    const target = new URL(item.finalUrl || item.url).origin;
    return target === new URL(record.initiator).origin && target === new URL(item.referrer).origin;
  } catch { return false; }
}

export async function handoff(api, item, download) {
  const pausedByUs = !item.paused;
  let created = false;
  try {
    if (pausedByUs) await api.pause(item.id);
    const current = await api.get(item.id);
    if (!current || current.state !== "in_progress") throw new Error("download_finished");
    // The IPC response waits for the ordinary Add Task form. No task or
    // credentials are persisted by the host just to transfer the request.
    const response = await api.send({ version: 1, action: "add_download", download });
    if (!response?.ok || !response.task_id || response.status !== "created") {
      if (pausedByUs) { try { await api.resume(item.id); } catch { /* Already finished. */ } }
      return response?.status === "cancelled" ? "cancelled" : "unavailable";
    }
    created = true;
    const beforeCancel = await api.get(item.id);
    if (!beforeCancel || beforeCancel.state !== "in_progress") return "browser_kept";
    await api.cancel(item.id);
    return "created"; // Starts through the same task controls as any normal task.
  } catch {
    if (pausedByUs) { try { await api.resume(item.id); } catch { /* Download may be complete. */ } }
    return created ? "browser_kept" : "unavailable";
  }
}
