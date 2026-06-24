use std::{collections::BTreeMap, net::IpAddr, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

pub type TaskId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DownloadKind {
    Http,
    Bt,
    Ftp,
    Sftp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Queued,
    Resolving,
    Downloading,
    Paused,
    Stopped,
    Completed,
    Seeding,
    Failed,
    Verifying,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskKind {
    Http(HttpTaskConfig),
    Bt(BtTaskConfig),
    Ftp(FtpTaskConfig),
    Sftp(SftpTaskConfig),
}

impl TaskKind {
    pub fn download_kind(&self) -> DownloadKind {
        match self {
            TaskKind::Http(_) => DownloadKind::Http,
            TaskKind::Bt(_) => DownloadKind::Bt,
            TaskKind::Ftp(_) => DownloadKind::Ftp,
            TaskKind::Sftp(_) => DownloadKind::Sftp,
        }
    }

    pub fn redacted(&self) -> Self {
        match self {
            TaskKind::Http(config) => TaskKind::Http(HttpTaskConfig {
                url: redact_url(&config.url),
                headers: config
                    .headers
                    .iter()
                    .map(|header| HeaderPair {
                        name: header.name.clone(),
                        value: if is_sensitive_header(&header.name) {
                            "<redacted>".to_string()
                        } else {
                            header.value.clone()
                        },
                    })
                    .collect(),
                ..config.clone()
            }),
            TaskKind::Bt(config) => TaskKind::Bt(BtTaskConfig {
                source: match &config.source {
                    BtSource::TorrentFile(path) => BtSource::TorrentFile(path.clone()),
                    BtSource::Magnet(value) => BtSource::Magnet(redact_magnet(value)),
                },
                trackers: config.trackers.iter().map(redact_url).collect(),
                ..config.clone()
            }),
            TaskKind::Ftp(config) => TaskKind::Ftp(FtpTaskConfig {
                url: redact_url(&config.url),
                ..config.clone()
            }),
            TaskKind::Sftp(config) => TaskKind::Sftp(SftpTaskConfig {
                url: redact_url(&config.url),
                ..config.clone()
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpTaskConfig {
    pub url: Url,
    pub method: HttpMethod,
    pub headers: Vec<HeaderPair>,
    pub max_connections: Option<u16>,
    pub min_split_size: Option<u64>,
    pub redirect_limit: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResourceMeta {
    pub final_url: Url,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_length: Option<u64>,
    pub supports_ranges: bool,
    pub temp_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpSegmentState {
    Pending,
    Downloading,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpSegment {
    pub index: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_bytes: u64,
    pub state: HttpSegmentState,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

impl HttpSegment {
    pub fn next_offset(&self) -> u64 {
        self.start_byte + self.downloaded_bytes
    }

    pub fn is_complete(&self) -> bool {
        self.downloaded_bytes > self.end_byte - self.start_byte
            && self.state == HttpSegmentState::Completed
    }
}

impl HttpTaskConfig {
    pub const DEFAULT_MAX_CONNECTIONS: u16 = 16;
    pub const DEFAULT_MIN_SPLIT_SIZE: u64 = 8 * 1024 * 1024;
    pub const DEFAULT_REDIRECT_LIMIT: u8 = 10;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtTaskConfig {
    pub source: BtSource,
    pub selected_files: Vec<u32>,
    pub trackers: Vec<Url>,
    pub max_connections: Option<u32>,
    pub share_ratio_limit: Option<f64>,
    pub enable_seeding: bool,
    pub anti_leech: AntiLeechConfig,
    pub ip_filter: IpFilterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BtSource {
    TorrentFile(PathBuf),
    Magnet(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AntiLeechConfig {
    pub blocked_client_names: Vec<String>,
    pub blocked_peer_id_prefixes: Vec<String>,
    pub block_suspicious_fast_disconnects: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IpFilterConfig {
    pub allow: Vec<IpRule>,
    pub deny: Vec<IpRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpRule {
    pub cidr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtpTaskConfig {
    pub url: Url,
    pub username: Option<String>,
    pub passive: bool,
    pub ftps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpTaskConfig {
    pub url: Url,
    pub username: Option<String>,
    pub private_key_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskCredentials {
    pub headers: Vec<HeaderPair>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub private_key_passphrase: Option<String>,
    pub extra: BTreeMap<String, String>,
}

impl TaskCredentials {
    pub fn redacted(&self) -> Self {
        let redact = |value: &Option<String>| value.as_ref().map(|_| "<redacted>".to_string());
        Self {
            headers: self
                .headers
                .iter()
                .map(|header| HeaderPair {
                    name: header.name.clone(),
                    value: if is_sensitive_header(&header.name) {
                        "<redacted>".to_string()
                    } else {
                        header.value.clone()
                    },
                })
                .collect(),
            username: self.username.clone(),
            password: redact(&self.password),
            private_key_passphrase: redact(&self.private_key_passphrase),
            extra: self
                .extra
                .iter()
                .map(|(key, value)| {
                    let value = if is_sensitive_key(key) {
                        "<redacted>".to_string()
                    } else {
                        value.clone()
                    };
                    (key.clone(), value)
                })
                .collect(),
        }
    }
}

impl DownloadTask {
    pub fn redacted(&self) -> Self {
        Self {
            kind: self.kind.redacted(),
            proxy: self.proxy.redacted(),
            ..self.clone()
        }
    }
}

impl TaskDetail {
    pub fn redacted(&self) -> Self {
        Self {
            task: self.task.redacted(),
            credentials: self.credentials.redacted(),
        }
    }
}

impl ProxyPolicy {
    pub fn redacted(&self) -> Self {
        match self {
            ProxyPolicy::Custom(config) => ProxyPolicy::Custom(ProxyConfig {
                url: redact_url(&config.url),
                username: config.username.clone(),
            }),
            other => other.clone(),
        }
    }
}

pub fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "cookie"
        || lower == "authorization"
        || lower == "proxy-authorization"
        || is_sensitive_key(&lower)
}

pub fn redact_url(url: &Url) -> Url {
    let mut redacted = url.clone();
    if redacted.password().is_some() {
        let _ = redacted.set_password(Some("<redacted>"));
    }
    let sensitive_pairs = redacted
        .query_pairs()
        .map(|(key, value)| {
            let value = if is_sensitive_key(&key) {
                "<redacted>".into()
            } else {
                value
            };
            (key.into_owned(), value.into_owned())
        })
        .collect::<Vec<_>>();
    if sensitive_pairs.is_empty() {
        return redacted;
    }
    redacted.set_query(None);
    {
        let mut pairs = redacted.query_pairs_mut();
        for (key, value) in sensitive_pairs {
            pairs.append_pair(&key, &value);
        }
    }
    redacted
}

fn redact_magnet(value: &str) -> String {
    match Url::parse(value) {
        Ok(url) => redact_url(&url).to_string(),
        Err(_) => "<redacted-magnet>".to_string(),
    }
}

pub fn is_sensitive_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("key")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ProxyPolicy {
    #[default]
    UseGlobal,
    System,
    Direct,
    Custom(ProxyConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub url: Url,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskRateLimit {
    pub download_bytes_per_second: Option<u64>,
    pub upload_bytes_per_second: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: TaskId,
    pub kind: TaskKind,
    pub save_dir: PathBuf,
    pub file_name: Option<String>,
    pub state: TaskState,
    pub limits: TaskRateLimit,
    pub proxy: ProxyPolicy,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskInput {
    pub kind: TaskKind,
    pub save_dir: PathBuf,
    pub file_name: Option<String>,
    pub limits: TaskRateLimit,
    pub proxy: ProxyPolicy,
    pub credentials: TaskCredentials,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskFilter {
    pub states: Vec<TaskState>,
    pub kinds: Vec<DownloadKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: TaskId,
    pub kind: DownloadKind,
    pub file_name: Option<String>,
    pub state: TaskState,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&DownloadTask> for TaskSummary {
    fn from(task: &DownloadTask) -> Self {
        Self {
            id: task.id,
            kind: task.kind.download_kind(),
            file_name: task.file_name.clone(),
            state: task.state.clone(),
            total_bytes: task.total_bytes,
            downloaded_bytes: task.downloaded_bytes,
            uploaded_bytes: task.uploaded_bytes,
            error: task.error.clone(),
            created_at: task.created_at,
            updated_at: task.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    pub task: DownloadTask,
    pub credentials: TaskCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSnapshot {
    pub download_limit: Option<u64>,
    pub upload_limit: Option<u64>,
    pub use_system_proxy: bool,
    pub bt_trackers: Vec<Url>,
    pub bt_ip_allow: Vec<String>,
    pub bt_ip_deny: Vec<String>,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            download_limit: None,
            upload_limit: None,
            use_system_proxy: true,
            bt_trackers: Vec::new(),
            bt_ip_allow: Vec::new(),
            bt_ip_deny: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpFilterDecision {
    Allow,
    Deny,
}

/// Per-file state for an active BitTorrent task, surfaced to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtFileState {
    pub index: u32,
    pub name: String,
    pub size: u64,
    pub downloaded: u64,
    pub selected: bool,
}

/// A snapshot of an active BitTorrent task's protocol-level state.
/// `piece_haves` is a packed bitfield (1 bit per piece, MSB-first within each
/// byte); `total_pieces` gives the bit count. `seed_ratio` is
/// `uploaded_bytes / total_bytes` when computable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BtStateSnapshot {
    pub files: Vec<BtFileState>,
    pub piece_haves: Vec<u8>,
    pub total_pieces: u32,
    pub seed_ratio: Option<f64>,
}

/// A file row in a pre-creation magnet preview. `selected` starts true for
/// every file; the UI toggles it before task creation to build
/// `BtTaskConfig::selected_files`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagnetPreviewFile {
    pub index: u32,
    pub name: String,
    pub size: u64,
    pub selected: bool,
}

/// Resolved metadata for a magnet link, produced before a task is created so
/// the user can preview and pick files. This is a read-only probe: it does not
/// persist a task or start a download. `info_hash` is the 40-char hex SHA-1 of
/// the torrent info dictionary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagnetPreview {
    pub name: Option<String>,
    pub info_hash: String,
    pub total_bytes: u64,
    pub files: Vec<MagnetPreviewFile>,
    pub trackers: Vec<String>,
}

pub fn apply_ip_filter(config: &IpFilterConfig, ip: IpAddr) -> IpFilterDecision {
    if config
        .allow
        .iter()
        .any(|rule| cidr_contains(&rule.cidr, ip).unwrap_or(false))
    {
        return IpFilterDecision::Allow;
    }
    if config
        .deny
        .iter()
        .any(|rule| cidr_contains(&rule.cidr, ip).unwrap_or(false))
    {
        return IpFilterDecision::Deny;
    }
    IpFilterDecision::Allow
}

pub fn cidr_contains(cidr: &str, ip: IpAddr) -> Option<bool> {
    let (base, prefix) = cidr.split_once('/').unwrap_or((cidr, ""));
    let base: IpAddr = base.parse().ok()?;
    if prefix.is_empty() {
        return Some(base == ip);
    }
    let prefix: u32 = prefix.parse().ok()?;
    match (base, ip) {
        (IpAddr::V4(base), IpAddr::V4(ip)) if prefix <= 32 => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            Some((u32::from(base) & mask) == (u32::from(ip) & mask))
        }
        (IpAddr::V6(base), IpAddr::V6(ip)) if prefix <= 128 => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            Some((u128::from(base) & mask) == (u128::from(ip) & mask))
        }
        _ => None,
    }
}
