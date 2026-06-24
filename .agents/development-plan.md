# Fluxion 开发计划

## 1. 开发策略

项目按能力闭环推进，而不是一次性铺开所有协议。第一目标是让 HTTP/HTTPS 下载从创建任务到完成文件形成可用闭环，并保证架构能承载 BT、FTP/SFTP 和 Chrome 插件。

优先级：

1. Core 基础架构。
2. HTTP 多线程分块下载。
3. Tauri App 基础任务管理。
4. 稳定性、断点续传、限速、代理和票据存储。
5. BT 下载。
6. FTP/SFTP。
7. Chrome 插件。

## 2. 里程碑

### M0：项目骨架与架构边界

目标：建立 App/Core 分层和 Rust workspace。

任务：

- 创建 workspace 目录结构。
- 创建 `fluxion-core` crate。
- 创建 `fluxion-http` crate。
- 创建 `fluxion-storage` crate。
- 创建 `fluxion-platform` crate。
- 创建 Tauri desktop app。
- 定义 Core API、任务模型、事件模型、错误模型。
- 定义 `DownloadEngine` trait。
- 定义 HTTP、BT、FTP、SFTP 的配置数据结构。

验收：

- workspace 能编译。
- App 能启动空界面。
- App 能调用 Core 的 mock command。
- Core 单元测试可以运行。

### M1：HTTP 单线程下载闭环

目标：完成最小可用 HTTP 下载。

任务：

- 实现 HTTP task 创建。
- 实现 HEAD 探测和 302 跟随。
- 实现文件名推断。
- 实现单线程 GET 下载。
- 实现临时文件写入和完成后重命名。
- 实现基础任务状态事件。
- 实现 SQLite 任务持久化。
- App 实现新建下载、任务列表、进度展示。

验收：

- 可以下载普通 HTTP/HTTPS 文件。
- 支持 302 跟随。
- App 可展示进度、速度和完成状态。
- App 重启后能看到历史任务。

### M2：HTTP Range 探测与多线程分块

目标：实现核心竞争力，多线程分块下载。

任务：

- 实现 `GET Range: bytes=0-0` 探测。
- 解析 `Content-Range`。
- 实现分块规划。
- 实现并发 worker。
- 实现位置写入。
- 实现分块进度持久化。
- 实现不支持 Range 的自动回落。
- 实现默认 16 线程和任务级最大线程数配置。
- 实现最终文件大小校验。
- 增加本地 HTTP 测试服务器覆盖 200/206/302/416 场景。

验收：

- 支持 Range 的文件使用多线程下载。
- 不支持 Range 的文件自动单线程。
- 大文件下载过程中内存占用稳定。
- 并发分块写入后的文件 hash 正确。

### M3：暂停、恢复、重试和限速

目标：提升 HTTP 下载稳定性。

任务：

- 实现任务暂停。
- 实现任务恢复。
- 实现任务停止。
- 实现分块级重试和指数退避。
- 实现全局下载限速。
- 实现任务级下载限速。
- 实现进度落库节流。
- 实现 App 操作按钮状态。
- 实现错误分类和用户可读错误信息。

验收：

- 下载中暂停后不继续消耗网络。
- 恢复后从已完成位置继续。
- App 退出重启后能继续未完成任务。
- 限速配置生效。
- 网络中断后按策略重试。

### M4：Headers、票据、系统代理

目标：支持需要登录态或特殊请求头的 HTTP 下载。

任务：

- 实现自定义 Headers。
- 实现 Cookie、Authorization 等敏感 Header 脱敏。
- 集成 macOS Keychain 或加密 SecretStore。
- 实现是否使用系统代理配置。
- 实现 App 的高级创建任务表单。
- 日志脱敏。

验收：

- 用户可以创建带 Cookie/Header 的任务。
- 敏感值不明文写入普通数据库。
- 日志不输出敏感 Header。
- 系统代理开关对 HTTP 下载生效。

### M5：App 体验完善

目标：让 macOS App 达到可日常使用的基本品质。

任务：

- 设计任务列表信息密度和视觉层级。
- 实现任务详情页或详情抽屉。
- 实现批量开始、暂停、删除。
- 实现保存路径选择。
- 实现设置页。
- 实现空状态、错误状态、完成状态。
- 实现下载完成后的打开文件和在 Finder 中显示。
- 增加端到端 UI 验证。

验收：

- 用户无需命令行即可完成 HTTP 下载管理。
- UI 状态清晰，不依赖刷新页面。
- 错误任务有明确原因和重试入口。

### M6：BT 下载基础

目标：支持种子和磁力链接创建下载任务。

任务：

- 评估并选择 BT crate 或确定自研范围。
- 实现 torrent metainfo 解析。
- 实现 magnet 解析。
- 实现 Tracker list 配置。
- 实现文件选择。
- 实现 piece 状态持久化。
- 实现基础下载和 hash 校验。
- App 实现新建 BT 任务、文件选择和 BT 详情页。

验收：

- 可以通过种子文件创建任务。
- 可以通过磁力链接创建任务。
- 创建时可以选择文件。
- 可以查看整体和文件级进度。

### M7：BT 高级能力

目标：补齐 BT 可配置能力。

任务：

- 实现全局和任务级最大连接数。
- 实现上传限速。
- 实现完成后做种。
- 实现分享率达到阈值后停止上传。
- 实现 UPnP/NAT-PMP。
- 实现自定义 Tracker list。
- 实现 IP 黑白名单和 CIDR 匹配。
- 实现反吸血客户端屏蔽规则。

验收：

- 完成任务可进入做种状态。
- 分享率限制有效。
- Tracker、连接数和 IP 过滤配置有效。
- App 可以展示做种速度和分享率。

### M8：FTP/SFTP

目标：扩展传统文件协议下载。

任务：

- 实现 FTP 单文件下载。
- 实现 FTP REST 续传。
- 实现 SFTP 单文件下载。
- 实现 SFTP 密码和私钥认证。
- 接入统一限速、任务状态和持久化。
- App 新建任务入口支持 FTP/SFTP。

验收：

- FTP/SFTP 任务能创建、暂停、恢复和完成。
- 任务级限速和全局限速生效。
- App 与 HTTP 任务使用同一套任务管理体验。

### M9：Chrome 插件

目标：接管浏览器下载。

任务：

- 实现 Chrome Extension manifest。
- 使用 downloads/webRequest 相关 API 捕获下载信息。
- 实现 Native Messaging host。
- App 接收插件任务。
- 继承 Cookie、User-Agent、Referer、Authorization 等 Headers。
- 实现用户确认流和自动接管设置。

验收：

- 浏览器下载可以发送到 Fluxion。
- 登录态资源可通过继承 Headers 下载。
- Cookie 等敏感信息只保存在本地并脱敏展示。

## 3. 推荐技术选型

### 3.1 Core

- Rust async runtime：Tokio。
- HTTP client：优先 `reqwest`，需要更细粒度控制时评估 `hyper`。
- TLS：优先 rustls，系统代理和证书兼容性需要单独验证。
- 数据库：SQLite。
- SQLite async：`sqlx` 或 `rusqlite` 加专用线程。
- 序列化：serde。
- 日志：tracing。
- 取消控制：tokio-util `CancellationToken`。

### 3.2 App

- Tauri 2。
- 前端框架可选 React、Svelte 或 Vue，建议选择团队最熟悉的一种。
- 状态管理使用轻量 store。
- UI 组件建议自建核心组件或使用成熟无障碍组件库。

### 3.3 平台能力

- macOS Keychain：用于敏感票据。
- macOS 系统代理读取：放在 `fluxion-platform`。
- 文件打开和 Finder 展示：由 Tauri shell/plugin 或平台层实现。

## 4. 关键实现顺序

HTTP 多线程下载建议按以下顺序实现：

1. 单线程下载。
2. HEAD 探测。
3. Range 探测。
4. 分块计划。
5. 位置写入。
6. 分块进度持久化。
7. 暂停恢复。
8. 重试。
9. 限速。
10. Headers 和票据。

这样每一步都有可验证产物，不会在并发下载尚未稳定时过早引入票据和代理复杂度。

## 5. 风险与对策

### 5.1 HTTP 服务端行为不一致

风险：

- HEAD 不支持。
- Range 被忽略。
- Content-Length 不准确。
- ETag 下载中变化。

对策：

- HEAD 失败后使用 Range GET 探测。
- 只在明确 206 时启用分块。
- 完成后校验文件长度。
- ETag/Last-Modified 变化时停止续传并提示用户重新下载。

### 5.2 并发写文件导致损坏

风险：

- 分块 offset 计算错误。
- 暂停恢复后重复写或漏写。

对策：

- 分块计划单元测试覆盖边界。
- 每个 segment 持久化 start/end/downloaded。
- 集成测试比较最终文件 hash。
- 位置写入封装在单一 writer 抽象中。

### 5.3 内存和 CPU 占用过高

风险：

- 每个 worker buffer 过大。
- UI 事件过于频繁。
- 进度频繁写库。

对策：

- 固定 buffer，默认 64 KiB 到 256 KiB。
- 进度事件节流。
- 数据库进度批量或周期写入。
- 默认并发 16，但根据文件大小减少分块数。

### 5.4 敏感信息泄露

风险：

- Cookie 明文入库。
- 日志输出 Authorization。
- 插件传递过多 Headers。

对策：

- SecretStore 存储敏感字段。
- 日志层统一脱敏。
- 插件只传递下载必要 Headers。
- 诊断导出默认脱敏。

### 5.5 BT 复杂度拖慢主线

风险：

- BT 协议、DHT、做种和反吸血实现周期长。

对策：

- 第一阶段只定义 BT 接口，不阻塞 HTTP。
- 单独评估 BT crate。
- BT 作为独立 `fluxion-bt` 接入 Core。

## 6. 测试计划

### 6.1 单元测试

- URL 和 Header 校验。
- Content-Disposition 文件名解析。
- Range 探测结果解析。
- 分块计划。
- 限速器。
- 错误分类。
- Secret 脱敏。

### 6.2 集成测试

使用本地 HTTP 测试服务模拟：

- 302 跳转。
- HEAD 失败。
- 支持 Range。
- 不支持 Range。
- Range 返回 416。
- 中途断连。
- 5xx 重试。
- 需要 Cookie 才能下载。

每个下载测试都验证：

- 任务状态。
- 进度。
- 最终文件大小。
- 最终文件 hash。

### 6.3 App 测试

- 创建任务表单校验。
- 任务操作按钮。
- 设置修改。
- 事件更新。
- App 重启后的任务恢复。

## 7. 第一阶段交付清单

第一阶段建议交付：

- Rust workspace 和 Tauri App 骨架。
- Core 任务 API。
- SQLite 持久化。
- HTTP 单线程和多线程下载。
- 302 跟随。
- Headers/Cookie 支持。
- Range 自动探测和回落。
- 暂停、恢复、停止、删除。
- 全局和任务级下载限速。
- macOS 系统代理开关。
- 基础 UI。
- 本地 HTTP 集成测试。

## 8. 建议的开发节奏

每个迭代控制在 1 到 2 周：

- Iteration 1：M0。
- Iteration 2：M1。
- Iteration 3：M2。
- Iteration 4：M3。
- Iteration 5：M4 + M5 基础。
- Iteration 6：M5 完善和稳定性修复。
- Iteration 7 之后：BT、FTP/SFTP、Chrome 插件按业务优先级推进。

## 9. 决策记录

当前建议决策：

- Core 用 Rust 实现，并作为 Tauri 后端依赖。
- HTTP 是第一优先协议。
- BT 不在第一阶段实现，但第一阶段保留接口。
- 任务持久化使用 SQLite。
- 敏感票据优先进入 macOS Keychain。
- Core 通过事件驱动 App 更新，不以轮询作为主路径。

