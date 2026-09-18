// Optional real Chromium check. The Native Messaging API is a test double;
// Rust tests separately exercise the real host and native Add Task form.
import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

test("Chromium capture: real webRequest cookies, threshold, unknown size, UI and fallback", {
  skip: !process.env.FLUXION_PLAYWRIGHT_MODULE,
  timeout: 90_000,
}, async () => {
  const { chromium } = await import(process.env.FLUXION_PLAYWRIGHT_MODULE);
  const profile = await mkdtemp(join(tmpdir(), "fluxion-chromium-"));
  const requests = [];
  const server = http.createServer((req, res) => {
    if (!req.url.startsWith("/file")) {
      res.writeHead(200, { "Content-Type": "text/html", "Set-Cookie": "session=fixture-cookie; HttpOnly; SameSite=Lax; Path=/" });
      res.end('<a href="/file-large" download>large</a> <a href="/file-small" download>small</a> <a href="/file-unknown" download>unknown</a> <a href="/file-visible">visible</a>');
      return;
    }
    requests.push({ url: req.url, cookie: req.headers.cookie });
    const size = req.url.includes("small") ? 256 * 1024 : 2 * 1024 * 1024;
    const headers = { "Content-Type": "application/octet-stream", "Content-Disposition": `attachment; filename="${req.url.slice(1)}.bin"` };
    if (!req.url.includes("unknown")) headers["Content-Length"] = size;
    res.writeHead(200, headers);
    let sent = 0;
    const timer = setInterval(() => {
      const chunk = Buffer.alloc(Math.min(8192, size - sent), 42);
      res.write(chunk); sent += chunk.length;
      if (sent >= size) { clearInterval(timer); res.end(); }
    }, 8);
    res.on("close", () => clearInterval(timer));
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const extension = fileURLToPath(new URL(".", import.meta.url));
  let context;
  try {
    context = await chromium.launchPersistentContext(profile, {
      channel: "chromium", headless: true, acceptDownloads: true,
      args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
    });
    const worker = context.serviceWorkers()[0] || await context.waitForEvent("serviceworker");
    assert.equal(new URL(worker.url()).host, "mkfokhcdpnpilifmdeecgkghkpicikaa");
    await worker.evaluate(() => {
      globalThis.messages = [];
      globalThis.fixtureEvents = [];
      chrome.downloads.onCreated.addListener(item => globalThis.fixtureEvents.push({ kind: "download", item }));
      chrome.webRequest.onBeforeSendHeaders.addListener(details => globalThis.fixtureEvents.push({ kind: "request", details }), { urls: ["http://127.0.0.1/*"] }, ["requestHeaders", "extraHeaders"]);
      chrome.webRequest.onSendHeaders.addListener(details => globalThis.fixtureEvents.push({ kind: "sent", details }), { urls: ["http://127.0.0.1/*"] }, ["requestHeaders", "extraHeaders"]);
      chrome.webRequest.onHeadersReceived.addListener(details => globalThis.fixtureEvents.push({ kind: "response", details }), { urls: ["http://127.0.0.1/*"] }, ["responseHeaders"]);
      chrome.runtime.sendNativeMessage = async (_host, message) => {
        globalThis.messages.push(message);
        if (globalThis.failAdd && message.action === "add_download") throw new Error("fixture host unavailable");
        if (globalThis.holdForm && message.action === "add_download") {
          return new Promise(resolve => { globalThis.decideForm = resolve; });
        }
        return { ok: true, status: "created", task_id: "fixture-task" };
      };
    });
    const setSettings = async value => {
      await worker.evaluate(async value => { await chrome.storage.local.set({ settings: value }); },
        { enabled: true, minimumMiB: 1, captureUnknown: false, extraHeaders: [], ...value });
      // Wait for storage.onChanged, which also discards prior request credentials.
      await new Promise(resolve => setTimeout(resolve, 100));
    };
    const waitMessage = async (action, count = 1) => {
      for (let i = 0; i < 200; i++) {
        const messages = await worker.evaluate(() => globalThis.messages);
        if (messages.filter(m => m.action === action).length >= count) return messages;
        await new Promise(resolve => setTimeout(resolve, 25));
      }
      throw new Error(`No ${action} message observed: ${JSON.stringify(await worker.evaluate(() => ({ messages: globalThis.messages, events: globalThis.fixtureEvents })))}`);
    };
    const page = await context.newPage();
    await setSettings({});
    await page.goto(origin);
    await worker.evaluate(() => { globalThis.holdForm = true; });
    const firstDownload = page.waitForEvent("download");
    await page.getByText("large", { exact: true }).click();
    const messages = await waitMessage("add_download");
    const pending = await worker.evaluate(async () => (await chrome.downloads.search({ state: "in_progress" }))[0]);
    assert.equal(pending.paused, true, "Browser stays paused while Add Task is open");
    await worker.evaluate(() => { globalThis.holdForm = false; globalThis.decideForm({ ok: true, status: "created", task_id: "fixture-task" }); });
    assert.notEqual(await (await firstDownload).failure(), null);
    const prepared = messages.find(m => m.action === "add_download");
    assert.equal(prepared.download.url, `${origin}/file-large`);
    assert.equal(prepared.download.filename, "file-large.bin");
    assert(prepared.download.headers.some(h => h.name === "cookie" && h.value.includes("session=fixture-cookie")));
    assert(prepared.download.headers.some(h => h.name === "referer" && h.value === `${origin}/`));
    assert.deepEqual(messages.map(m => m.action), ["add_download"]);
    assert(requests.some(r => r.cookie === "session=fixture-cookie"));

    const smallEvent = page.waitForEvent("download");
    await page.getByText("small", { exact: true }).click();
    assert.equal(await (await smallEvent).failure(), null);
    assert.equal((await worker.evaluate(() => globalThis.messages)).length, 1);

    const unknownEvent = page.waitForEvent("download");
    await page.getByText("unknown", { exact: true }).click();
    assert.equal(await (await unknownEvent).failure(), null);
    assert.equal((await worker.evaluate(() => globalThis.messages)).length, 1);

    await setSettings({ captureUnknown: true });
    await page.getByText("unknown", { exact: true }).click();
    await waitMessage("add_download", 2);

    await setSettings({});
    await page.getByText("visible", { exact: true }).click();
    const visibleMessages = await waitMessage("add_download", 3);
    const visible = visibleMessages.find(m => m.download?.url === `${origin}/file-visible`);
    assert(visible.download.headers.some(h => h.name === "cookie" && h.value.includes("session=fixture-cookie")));

    await setSettings({});
    await worker.evaluate(() => { globalThis.failAdd = true; });
    const fallbackEvent = page.waitForEvent("download");
    await page.getByText("large", { exact: true }).click();
    assert.equal(await (await fallbackEvent).failure(), null);
    await waitMessage("add_download", 4);

    await setSettings({});
    await worker.evaluate(() => { globalThis.failAdd = false; globalThis.holdForm = true; });
    const cancelledEvent = page.waitForEvent("download");
    await page.getByText("large", { exact: true }).click();
    await waitMessage("add_download", 5);
    await worker.evaluate(() => globalThis.decideForm({ ok: false, status: "cancelled" }));
    assert.equal(await (await cancelledEvent).failure(), null, "Cancel form resumes the browser");

    const options = await context.newPage();
    await options.goto(`chrome-extension://${new URL(worker.url()).host}/options.html`);
    await options.getByLabel("小于此大小时，继续用浏览器下载").fill("2.5");
    await options.getByRole("button", { name: "保存设置" }).click();
    await options.getByRole("status").filter({ hasText: "设置已保存" }).waitFor();
    assert.equal(await worker.evaluate(async () => (await chrome.storage.local.get("settings")).settings.minimumMiB), 2.5);
    await options.setViewportSize({ width: 360, height: 740 });
    assert(await options.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    if (process.env.FLUXION_BROWSER_SCREENSHOT) await options.screenshot({ path: resolve(process.env.FLUXION_BROWSER_SCREENSHOT), fullPage: true });
  } finally {
    await context?.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await rm(profile, { recursive: true, force: true });
  }
});
