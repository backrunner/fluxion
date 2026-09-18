import { settingsFrom } from "./policy.mjs";
const $ = id => document.getElementById(id);
const send = message => chrome.runtime.sendMessage(message);
const status = text => { $("status").textContent = text; };

async function load() {
  const data = await send({ action: "status" });
  $("enabled").checked = data.settings.enabled;
  $("minimum").value = data.settings.minimumMiB;
  $("unknown").checked = data.settings.captureUnknown;
  $("headers").value = data.settings.extraHeaders.join(", ");
  status(data.lastStatus);
  if (!data.downloads.length) $("downloads").textContent = "暂无进行中的下载";
  for (const item of data.downloads) {
    const row = document.createElement("div"); row.className = "download";
    const label = document.createElement("span"); label.textContent = item.filename; label.title = item.filename;
    const button = document.createElement("button"); button.textContent = "交给 Fluxion"; button.className = "secondary";
    button.addEventListener("click", async () => {
      button.disabled = true;
      try { status((await send({ action: "capture", id: item.id })).lastStatus); }
      catch { status("操作未完成，下载仍在浏览器中。"); }
      finally { button.disabled = false; }
    });
    row.append(label, button); $("downloads").append(row);
  }
}
$("settings").addEventListener("submit", async event => {
  event.preventDefault();
  try {
    const settings = settingsFrom({ enabled: $("enabled").checked, minimumMiB: $("minimum").value,
      captureUnknown: $("unknown").checked, extraHeaders: $("headers").value.split(",") });
    const result = await send({ action: "save", settings });
    status(result.ok ? "设置已保存，对新下载生效。" : "未能保存设置，请重试。");
  } catch (error) { status(error.message); }
});
$("connect").addEventListener("click", async () => {
  $("connect").disabled = true; status("正在连接 Fluxion…");
  try { status((await send({ action: "ping" })).ok ? "已连接 Fluxion。" : "无法连接。请先打开新版 Fluxion，或按安装说明注册本地连接程序。"); }
  catch { status("连接失败，请重新打开扩展。"); }
  finally { $("connect").disabled = false; }
});
load().catch(() => status("无法读取扩展状态，请重新打开。"));
