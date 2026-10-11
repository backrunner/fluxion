---
title: "发行状态"
description: "Fluxion 正在为首次稳定发行做准备。"
---

## 正在准备首次发行

Fluxion 仍处于积极开发阶段，**目前尚未在 Mac App Store 上架**。此网站暂未提供 App Store 购买入口或 TestFlight 邀请。

我们正在准备采用沙盒的 Mac App Store 版与直接发行版。正式发行渠道、价格和支持的 macOS 版本确认后，会在此公布。请以已发布的信息为准。

## 两种发行渠道

| 功能 | Mac App Store 版 | 直接发行版 |
| --- | --- | --- |
| HTTP、HTTPS、BitTorrent、FTP、SFTP | 共用传输引擎 | 共用传输引擎 |
| 应用更新 | 通过 Apple | 原生签名更新 |
| 下载目录访问 | App Sandbox 与用户选择的目录 | 常规文件系统权限 |
| 浏览器捕获 | 不包含 | 可选 Chromium 扩展与本地任务交接 |
| CLI 与守护进程 | 独立于商店应用 | 可从源码构建 |

暂不支持 FTPS。浏览器捕获不属于计划中的商店版。开发构建不等于已通过 Apple 审核的发行版本。

## 关注开发进展

[源码仓库](https://github.com/backrunner/fluxion)提供构建说明、变更与问题跟踪。开发版本的存储格式或协议行为可能变化。

查看[首次下载指南](/docs/zh/downloading)和[协议说明](/docs/zh/protocols)，了解当前行为。
