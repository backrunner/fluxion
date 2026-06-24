# Fluxion 模块设计文档

## 1. 总体架构

Fluxion 拆分为 Fluxion App 和 Fluxion Core。

```text
fluxion/
  apps/
    desktop/                 # Tauri App
  crates/
    fluxion-core/            # 核心任务模型、调度、事件、限速、持久化抽象
    fluxion-http/            # HTTP/HTTPS 下载引擎
    fluxion-bt/              # BT 下载引擎，后续实现
    fluxion-ftp/             # FTP 下载引擎，后续实现
    fluxion-sftp/            # SFTP 下载引擎，后续实现
    fluxion-storage/         # SQLite、元数据、分块状态、Keychain 适配
    fluxion-platform/        # macOS 系统代理、Keychain、文件系统平台能力
    fluxion-tauri-bridge/    # Tauri commands 和 Core API 适配
```

第一阶段可以只创建必要 crate：

- `fluxion-core`
- `fluxion-http`
- `fluxion-storage`
- `fluxion-platform`
- `apps/desktop`

BT、FTP、SFTP 可以先定义接口和数据结构，实际引擎按阶段补齐。

## 2. 设计原则

- Core 不依赖 Tauri UI。
- App 只通过 Core API 和事件订阅访问下载能力。
- 每种协议实现为独立下载引擎。
- 任务调度、限速、代理、持久化、事件通知由 Core 统一管理。
- 下载过程流式处理，避免把文件内容加载到内存。
- 敏感票据和普通任务元数据分离存储。

## 3. Core 分层

```text
Fluxion App
  |
  | Tauri Commands / Event Bridge
  v
Fluxion Core API
  |
  +-- Task Manager
  +-- Scheduler
  +-- Engine Registry
  +-- Rate Limiter
  +-- Event Bus
  +-- Storage
  +-- Platform Services
        |
        +-- HTTP Engine
        +-- BT Engine
        +-- FTP Engine
        +-- SFTP Engine
```

### 3.1 Core API

Core 对 App 暴露稳定接口：

```rust
pub struct FluxionCore {
    task_manager: TaskManager,
    scheduler: Scheduler,
    events: EventBus,
}

impl FluxionCore {
    pub async fn create_task(&self, input: CreateTaskInput) -> Result<TaskId>;
    pub async fn start_task(&self, task_id: TaskId) -> Result<()>;
    pub async fn pause_task(&self, task_id: TaskId) -> Result<()>;
    pub async fn stop_task(&self, task_id: TaskId) -> Result<()>;
    pub async fn delete_task(&self, task_id: TaskId, delete_files: bool) -> Result<()>;
    pub async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<TaskSummary>>;
    pub async fn get_task(&self, task_id: TaskId) -> Result<TaskDetail>;
    pub fn subscribe(&self) -> EventStream;
}
```

### 3.2 任务模型

核心数据结构：

```rust
pub enum TaskKind {
    Http(HttpTaskConfig),
    Bt(BtTaskConfig),
    Ftp(FtpTaskConfig),
    Sftp(SftpTaskConfig),
}

pub struct DownloadTask {
    pub id: TaskId,
    pub kind: TaskKind,
    pub save_dir: PathBuf,
    pub file_name: Option<String>,
    pub state: TaskState,
    pub limits: TaskRateLimit,
    pub proxy: ProxyPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

任务状态由 Core 管理，协议引擎只能通过受控接口提交进度、错误和完成事件。

### 3.3 下载引擎接口

协议引擎使用统一 trait：

```rust
#[async_trait]
pub trait DownloadEngine: Send + Sync {
    fn kind(&self) -> DownloadKind;

    async fn prepare(&self, ctx: EngineContext, task: DownloadTask) -> Result<PreparedTask>;

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit>;
}
```

`prepare` 负责探测资源和生成执行计划。`run` 负责执行下载。

`TaskControl` 提供：

- pause token。
- stop token。
- cancellation token。
- 动态限速配置读取。

`EngineContext` 提供：

- 事件发送。
- 存储接口。
- 全局限速器。
- 平台服务。
- HTTP client factory 或网络配置。

### 3.4 事件总线

Core 向 App 发出事件：

```rust
pub enum CoreEvent {
    TaskCreated(TaskSummary),
    TaskStateChanged { task_id: TaskId, state: TaskState },
    TaskProgress(TaskProgress),
    TaskSpeed(TaskSpeed),
    TaskError(TaskErrorEvent),
    TaskCompleted { task_id: TaskId, file_path: PathBuf },
    SettingsChanged(SettingsSnapshot),
}
```

事件需要节流：

- 进度事件建议 250ms 到 500ms 聚合一次。
- 速度事件建议 1s 聚合一次。
- 状态变更和错误事件即时发送。

## 4. HTTP 下载引擎设计

### 4.1 模块结构

```text
fluxion-http/
  src/
    lib.rs
    engine.rs              # DownloadEngine 实现
    probe.rs               # HEAD / Range 探测
    planner.rs             # 分块计划生成
    worker.rs              # 分块下载 worker
    writer.rs              # 文件预分配和位置写入
    retry.rs               # 重试策略
    headers.rs             # Header 和敏感字段处理
```

### 4.2 HTTP 任务配置

```rust
pub struct HttpTaskConfig {
    pub url: Url,
    pub method: HttpMethod,
    pub headers: HeaderMap,
    pub credential_ref: Option<SecretRef>,
    pub max_connections: Option<u16>,
    pub min_split_size: Option<u64>,
    pub redirect_policy: RedirectPolicy,
}
```

默认值：

- `method`: GET。
- `max_connections`: 16。
- `min_split_size`: 8 MiB。
- `redirect_policy`: 最多跟随 10 次。

### 4.3 探测流程

```text
Create HTTP task
  |
  v
HEAD request with redirects
  |
  +-- success and enough metadata
  |      |
  |      v
  |   Evaluate Content-Length / Accept-Ranges / ETag
  |
  +-- unsupported / blocked / missing metadata
         |
         v
      GET Range: bytes=0-0
         |
         v
      Evaluate 206 and Content-Range
```

启用多线程分块条件：

- 最终 URL 可访问。
- 文件总长度已知。
- Range 探测返回 `206 Partial Content`。
- `Content-Range` 能解析出总长度。
- 文件大小大于最小分块阈值。

回落单线程条件：

- 无法确定总长度。
- Range 探测失败。
- 服务端忽略 Range 并返回 `200 OK`。
- 文件过小。
- 用户设置最大连接数为 1。

### 4.4 分块规划

分块目标：

- 并发数不超过用户配置和全局上限。
- 每块大小不小于 `min_split_size`。
- 对大文件使用均匀范围。

示例：

```text
file_size = 10 GiB
max_connections = 16
range 0:             0 .. 671088639
range 1:     671088640 .. 1342177279
...
range 15:  10066329600 .. 10737418239
```

持久化每个分块：

- `start`
- `end`
- `downloaded`
- `state`
- `retry_count`
- `last_error`

暂停恢复时，每个分块从 `start + downloaded` 继续请求。

### 4.5 文件写入

要求：

- 使用临时文件路径，例如 `filename.fluxionpart`。
- 下载前设置目标文件长度。
- macOS 可以通过平台层尝试预分配空间；失败时退化为 `set_len`。
- 分块 worker 按 offset 写入。
- 每次写入后更新内存进度，周期性落盘。
- 完成后校验文件长度并原子移动到最终路径。

推荐实现：

- worker 从网络流读取固定大小 buffer。
- 写入使用独立 writer 抽象，内部可使用阻塞线程池执行 `write_at`。
- 不在内存中聚合大块数据。

### 4.6 重试策略

可重试错误：

- 网络超时。
- 连接断开。
- 5xx。
- 429。
- 临时 DNS 错误。

不可重试错误：

- 401/403，除非用户更新票据。
- 404。
- 416 且无法用当前元数据恢复。
- 磁盘空间不足。
- 路径无权限。

策略：

- 指数退避。
- 单分块最大重试次数默认 5。
- 重试后仍失败则任务进入 `Failed`。

### 4.7 Header 与票据

普通 Headers 可以存在任务元数据中。敏感 Headers 需要脱敏：

- `Cookie`
- `Authorization`
- `Proxy-Authorization`
- 自定义匹配 `token`、`secret`、`key` 的 Header

敏感值存入 Keychain 或加密存储，任务配置只保存 `SecretRef`。

## 5. BT 下载引擎设计

### 5.1 实现策略

BT 协议复杂度高，建议优先评估成熟 Rust crate，再决定是否自研关键组件。目标是把 BT 能力封装在 `fluxion-bt`，不污染 Core。

需要重点验证：

- 磁力链接解析。
- DHT。
- Tracker。
- Peer wire protocol。
- Piece 校验。
- 文件选择。
- 做种。
- 限速和连接数控制。
- 客户端识别与屏蔽。

### 5.2 BT 模块

```text
fluxion-bt/
  src/
    lib.rs
    engine.rs
    metainfo.rs
    magnet.rs
    tracker.rs
    peer.rs
    piece_store.rs
    file_selection.rs
    seeding.rs
    anti_leech.rs
    ip_filter.rs
```

### 5.3 BT 任务配置

```rust
pub struct BtTaskConfig {
    pub source: BtSource,
    pub selected_files: Vec<FileIndex>,
    pub trackers: Vec<Url>,
    pub max_connections: Option<u32>,
    pub share_ratio_limit: Option<f64>,
    pub enable_seeding: bool,
}

pub enum BtSource {
    TorrentFile(PathBuf),
    Magnet(String),
}
```

### 5.4 Piece 与文件状态

BT 需要维护：

- piece 总数。
- 每个 piece 的状态。
- 每个文件映射到哪些 piece。
- 每个文件的完成字节数。
- piece hash 校验结果。

App 读取聚合后的状态，不直接解析 BT 内部数据。

### 5.5 反吸血与 IP 过滤

反吸血配置：

- 客户端名称黑名单。
- Peer ID 前缀黑名单。
- 行为规则：只下载不上传、频繁断连等。

IP 过滤配置：

- 黑名单 CIDR。
- 白名单 CIDR。
- 单个 IP。
- 规则优先级：白名单优先或黑名单优先需要在设置中明确。

## 6. FTP/SFTP 下载引擎设计

FTP/SFTP 使用同样的 `DownloadEngine` 接口。

FTP 重点：

- 被动模式优先。
- 支持 REST 断点续传。
- 用户名密码认证。

SFTP 重点：

- 用户名密码认证。
- 私钥认证。
- 支持远程文件 seek 后续传。

初期只支持单文件下载。目录递归下载作为增强能力。

## 7. Storage 设计

### 7.1 数据库

建议使用 SQLite。表结构初稿：

```text
tasks
  id
  kind
  state
  save_dir
  file_name
  total_bytes
  downloaded_bytes
  uploaded_bytes
  task_limit_download
  task_limit_upload
  proxy_policy
  created_at
  updated_at
  completed_at

http_tasks
  task_id
  original_url
  final_url
  method
  headers_json
  secret_ref
  etag
  last_modified
  content_length
  supports_ranges
  max_connections
  temp_path

http_segments
  task_id
  segment_index
  start_byte
  end_byte
  downloaded_bytes
  state
  retry_count
  last_error

settings
  key
  value_json
```

BT 后续新增：

```text
bt_tasks
bt_files
bt_pieces
bt_trackers
bt_peers_snapshot
ip_filter_rules
```

### 7.2 持久化策略

- 任务创建立即落库。
- 状态变更立即落库。
- 进度按周期落库，例如 1s 或下载固定字节数后。
- App 退出前尝试 flush。
- 崩溃恢复以数据库中的 segment 状态为准。

### 7.3 敏感信息

macOS 优先使用 Keychain：

```rust
pub trait SecretStore {
    async fn put(&self, scope: SecretScope, value: SecretValue) -> Result<SecretRef>;
    async fn get(&self, secret_ref: &SecretRef) -> Result<SecretValue>;
    async fn delete(&self, secret_ref: &SecretRef) -> Result<()>;
}
```

## 8. 限速器设计

使用令牌桶：

- `GlobalDownloadLimiter`
- `GlobalUploadLimiter`
- `TaskDownloadLimiter`
- `TaskUploadLimiter`

下载 worker 在读取或写入前获取令牌。为减少开销，可以按 buffer 大小申请令牌。

```rust
pub trait RateLimiter {
    async fn acquire(&self, bytes: u64);
    fn update_limit(&self, bytes_per_second: Option<u64>);
}
```

限速顺序：

1. 任务级 limiter。
2. 全局 limiter。

这样任务不能超过自己的限制，也不能突破全局限制。

## 9. 代理设计

```rust
pub enum ProxyPolicy {
    UseGlobal,
    System,
    Direct,
    Custom(ProxyConfig),
}
```

全局配置：

```rust
pub struct NetworkSettings {
    pub use_system_proxy: bool,
    pub custom_proxy: Option<ProxyConfig>,
}
```

第一阶段至少支持：

- 使用系统代理。
- 直连。

`fluxion-platform` 负责 macOS 系统代理读取，HTTP 引擎只消费统一的代理配置。

## 10. Tauri App 设计

### 10.1 Tauri Commands

```rust
#[tauri::command]
async fn create_task(input: CreateTaskInput) -> Result<TaskIdDto>;

#[tauri::command]
async fn start_task(task_id: String) -> Result<()>;

#[tauri::command]
async fn pause_task(task_id: String) -> Result<()>;

#[tauri::command]
async fn stop_task(task_id: String) -> Result<()>;

#[tauri::command]
async fn delete_task(task_id: String, delete_files: bool) -> Result<()>;

#[tauri::command]
async fn list_tasks(filter: TaskFilterDto) -> Result<Vec<TaskSummaryDto>>;

#[tauri::command]
async fn get_task(task_id: String) -> Result<TaskDetailDto>;

#[tauri::command]
async fn update_settings(input: SettingsInput) -> Result<()>;
```

### 10.2 前端状态

App 前端建议维护：

- `tasksStore`：任务列表。
- `taskDetailsStore`：当前查看任务详情。
- `settingsStore`：全局配置。
- `eventsStore`：Core 事件订阅和合并。

UI 不应依赖轮询作为主路径，Core 事件驱动更新；列表首次加载和异常恢复可使用查询接口。

### 10.3 页面结构

第一阶段：

- Downloads：任务列表主界面。
- Task Detail：任务详情。
- New Download：创建 HTTP 下载任务。
- Settings：全局设置。

后续：

- New Torrent。
- Torrent Detail。
- Peer/Tracker Detail。
- Chrome Integration Setup。

## 11. Chrome 插件集成设计

建议使用 Native Messaging：

```text
Chrome Extension
  |
  | Native Messaging
  v
Fluxion Native Host
  |
  v
Fluxion App / Core
```

插件发送：

```json
{
  "type": "create_http_task",
  "url": "https://example.com/file.zip",
  "headers": {
    "Cookie": "...",
    "User-Agent": "...",
    "Referer": "..."
  },
  "suggested_file_name": "file.zip"
}
```

App 接收后：

- 展示确认弹窗或按用户设置自动创建任务。
- 敏感 Headers 写入 SecretStore。
- 普通 Headers 写入任务配置。

## 12. 错误模型

Core 统一错误类型：

```rust
pub enum FluxionErrorKind {
    Network,
    HttpStatus,
    Unauthorized,
    NotFound,
    RangeNotSupported,
    DiskFull,
    PermissionDenied,
    InvalidConfig,
    Storage,
    Cancelled,
    Unknown,
}
```

错误需要同时支持：

- 面向开发者的详细日志。
- 面向用户的短消息。
- 可选的恢复建议，例如更新 Cookie、检查磁盘空间、重试。

## 13. 测试设计

Core 测试：

- HTTP Range 探测。
- HEAD 不可用时 fallback。
- 不支持 Range 时单线程。
- 分块计划正确性。
- 暂停恢复。
- ETag 变化处理。
- 限速器。
- 持久化恢复。

集成测试：

- 本地 HTTP 测试服务器。
- 模拟 302。
- 模拟 206。
- 模拟忽略 Range。
- 模拟连接中断。
- 验证最终文件 hash。

App 测试：

- Tauri command 调用。
- 任务列表状态更新。
- 创建任务表单校验。
- 任务操作按钮状态。

