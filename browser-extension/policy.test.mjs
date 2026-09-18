import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { DEFAULTS, settingsFrom, selectHeaders, responseSize, responseFilename, matchRequest, captureDecision, handoff, canRecoverCookies } from "./policy.mjs";

const now = Date.now();
const item = { id: 7, url: "https://site.test/redirect", finalUrl: "https://cdn.test/file", state: "in_progress", totalBytes: 10 * 1024 * 1024, startTime: new Date(now).toISOString(), referrer: "https://site.test/" };
const record = { url: item.finalUrl, method: "GET", status: 200, size: item.totalBytes, responseTime: now, headers: [{ name: "referer", value: item.referrer }] };
const settings = { ...DEFAULTS, enabled: true };

test("size boundary, disabled, unknown, unsupported and manual policies", () => {
  assert.equal(captureDecision(item, DEFAULTS, record), "skip");
  assert.equal(captureDecision(item, settings, record), "capture");
  assert.equal(captureDecision({ ...item, totalBytes: item.totalBytes - 1 }, settings, record), "small");
  assert.equal(captureDecision({ ...item, totalBytes: 0 }, { ...settings, minimumMiB: 0 }, record), "capture");
  assert.equal(captureDecision({ ...item, totalBytes: -1 }, settings, { ...record, size: null }), "unknown");
  assert.equal(captureDecision({ ...item, totalBytes: 0 }, settings, { ...record, size: null }), "unknown");
  assert.equal(captureDecision({ ...item, totalBytes: 0 }, settings, { ...record, size: 0 }), "small");
  assert.equal(captureDecision({ ...item, totalBytes: -1 }, { ...settings, captureUnknown: true }, { ...record, size: null }), "capture");
  assert.equal(captureDecision({ ...item, totalBytes: -1 }, settings, record), "capture");
  assert.equal(captureDecision(item, DEFAULTS, record, true), "capture");
  assert.equal(captureDecision(item, settings, { ...record, method: "POST" }), "unmatched");
  assert.equal(captureDecision(item, settings, { ...record, status: 206 }), "unmatched");
  assert.equal(captureDecision({ ...item, finalUrl: "blob:https://site.test/x" }, settings, record), "skip");
  assert.equal(captureDecision({ ...item, incognito: true }, settings, record), "skip");
  assert.equal(captureDecision({ ...item, paused: true }, settings, record), "skip");
  assert.equal(captureDecision({ ...item, state: "complete" }, settings, record, true), "skip");
});

test("hidden-header cookie recovery requires the exact top-level origin", () => {
  const hidden = { ...record, headersVisible: false, headers: [], type: "other", initiator: "https://cdn.test", tabId: 1, frameId: 0 };
  const sameOriginItem = { ...item, referrer: "https://cdn.test/page" };
  assert.equal(canRecoverCookies(sameOriginItem, hidden), true);
  assert.equal(matchRequest(sameOriginItem, new Map([[1, hidden]]), now), hidden);
  assert.equal(canRecoverCookies(item, hidden), false);
  for (const override of [{ frameId: 1 }, { tabId: -1 }, { authRequired: true }, { incognito: true }, { initiator: "https://other.test" }, { type: "xmlhttprequest" }]) {
    assert.equal(canRecoverCookies(sameOriginItem, { ...hidden, ...override }), false);
  }
  assert.equal(responseFilename([{ name: "Content-Disposition", value: 'attachment; filename="hello world.bin"' }]), "hello world.bin");
  assert.equal(responseFilename([{ name: "Content-Disposition", value: "attachment; filename*=UTF-8''%E4%B8%AD%E6%96%87.zip" }]), "中文.zip");
});

test("only the unique final request can contribute credentials", () => {
  assert.equal(matchRequest(item, new Map([[1, record]]), now), record);
  assert.equal(matchRequest(item, new Map([[1, { ...record, url: item.url }]]), now), null);
  assert.equal(matchRequest(item, new Map([[1, record], [2, { ...record }]]), now), null);
  assert.equal(matchRequest(item, new Map([[1, record], [2, { ...record, downloadId: 99 }]]), now), record);
  assert.equal(matchRequest(item, new Map([[1, { ...record, headers: [] }]]), now), null);
  assert.equal(matchRequest(item, new Map([[1, record]]), now + 61_000), null);
  assert.equal(matchRequest({ ...item, startTime: new Date(now + 20_000).toISOString() }, new Map([[1, record]]), now), null);
});

test("headers are minimal and extra auth headers require explicit names", () => {
  const input = ["Cookie", "Authorization", "Referer", "User-Agent", "X-Api-Key", "Host", "Range", "Proxy-Authorization", "Sec-Fetch-Site"].map(name => ({ name, value: "value" }));
  assert.deepEqual(selectHeaders(input).map(h => h.name), ["cookie", "authorization", "referer", "user-agent"]);
  assert.equal(selectHeaders(input, ["x-api-key"]).at(-1).name, "x-api-key");
  assert.deepEqual(selectHeaders([{ name: "Cookie", value: "a=1" }, { name: "cookie", value: "b=2" }]), [{ name: "cookie", value: "a=1; b=2" }]);
  assert.throws(() => selectHeaders([{ name: "Authorization", value: "a" }, { name: "authorization", value: "b" }]));
  assert.equal(responseSize([{ name: "Content-Length", value: "9000" }]), 9000);
  assert.equal(responseSize([{ name: "Content-Length", value: "9000" }, { name: "Content-Encoding", value: "gzip" }]), null);
  for (const minimumMiB of [-1, Infinity, NaN, "x"]) assert.throws(() => settingsFrom({ minimumMiB }));
  assert.throws(() => settingsFrom({ extraHeaders: ["Cookie"] }));
  assert.throws(() => settingsFrom({ extraHeaders: ["X-Forwarded-For"] }));
});

function fakeApi(fail) {
  const calls = [];
  return { calls,
    send: async message => {
      calls.push(message.action);
      if (fail === message.action) throw new Error("private URL must not escape");
      return { ok: true, status: "created", task_id: "task" };
    },
    get: async () => item,
    ...Object.fromEntries(["pause", "resume", "cancel"].map(name => [name, async () => {
      calls.push(name); if (fail === name) throw new Error(name);
    }])) };
}
test("handoff waits for form confirmation before cancelling the browser", async () => {
  const api = fakeApi();
  let decide;
  api.send = message => {
    api.calls.push(message.action);
    return new Promise(resolve => { decide = resolve; });
  };
  const pending = handoff(api, item, {});
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(api.calls, ["pause", "add_download"]);
  decide({ ok: true, status: "created", task_id: "task" });
  assert.equal(await pending, "created");
  assert.deepEqual(api.calls, ["pause", "add_download", "cancel"]);
});
test("cancelling the form restores the browser without creating or starting tasks", async () => {
  const api = fakeApi();
  api.send = async message => { api.calls.push(message.action); return { ok: false, status: "cancelled" }; };
  assert.equal(await handoff(api, item, {}), "cancelled");
  assert.deepEqual(api.calls, ["pause", "add_download", "resume"]);
});
test("IPC failure retains the browser download", async () => {
  const api = fakeApi("add_download");
  assert.equal(await handoff(api, item, {}), "unavailable");
  assert.deepEqual(api.calls, ["pause", "add_download", "resume"]);
});
test("failed browser cancellation leaves the confirmed ordinary task queued", async () => {
  const api = fakeApi("cancel");
  assert.equal(await handoff(api, item, {}), "browser_kept");
  assert.deepEqual(api.calls, ["pause", "add_download", "cancel", "resume"]);
});
test("manual paused download stays paused on form cancellation", async () => {
  const api = fakeApi();
  api.send = async () => ({ ok: false, status: "cancelled" });
  assert.equal(await handoff(api, { ...item, paused: true }, {}), "cancelled");
  assert.deepEqual(api.calls, []);
});
test("a download that cannot pause never reaches Fluxion", async () => {
  const api = fakeApi("pause");
  await handoff(api, item, {});
  assert.deepEqual(api.calls, ["pause", "resume"]);
});
test("manifest public key matches the host's origin allowlist", () => {
  const manifest = JSON.parse(readFileSync(new URL("manifest.json", import.meta.url)));
  const hash = createHash("sha256").update(Buffer.from(manifest.key, "base64")).digest().subarray(0, 16);
  const id = [...hash].map(b => String.fromCharCode(97 + (b >> 4), 97 + (b & 15))).join("");
  assert.equal(id, "mkfokhcdpnpilifmdeecgkghkpicikaa");
  assert.equal(manifest.manifest_version, 3);
});
