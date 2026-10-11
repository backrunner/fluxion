use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use fluxion_core::{
    DownloadEngine, DownloadKind, EngineContext, EngineExit, FluxionError, FluxionErrorKind,
    PreparedTask, Result, SECRET_SFTP_SOURCE_URL, SftpTaskConfig, TaskControl, TaskKind,
};
use russh::client;
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use url::Url;

const BUFFER_SIZE: usize = 128 * 1024;

pub struct SftpEngine {
    known_hosts: PathBuf,
}

impl SftpEngine {
    pub fn new() -> Self {
        Self::with_known_hosts(known_hosts_path())
    }

    pub fn with_known_hosts(path: PathBuf) -> Self {
        Self { known_hosts: path }
    }
}

impl Default for SftpEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DownloadEngine for SftpEngine {
    fn kind(&self) -> DownloadKind {
        DownloadKind::Sftp
    }

    async fn prepare(&self, ctx: EngineContext, mut task: PreparedTask) -> Result<PreparedTask> {
        let TaskKind::Sftp(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an SFTP task",
            ));
        };
        let name = task
            .task
            .file_name
            .clone()
            .or_else(|| inferred_file_name(&stored_config.url))
            .unwrap_or_else(|| "download".to_string());
        task.task.file_name = Some(fluxion_core::sanitize_file_name(&name));
        ctx.storage.update_task(task.task.clone()).await?;
        Ok(task)
    }

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit> {
        let TaskKind::Sftp(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an SFTP task",
            ));
        };
        let mut config = stored_config.clone();
        if let Some(url) = task.credentials.extra.get(SECRET_SFTP_SOURCE_URL) {
            config.url = Url::parse(url).map_err(|error| {
                FluxionError::new(
                    FluxionErrorKind::InvalidConfig,
                    format!("stored SFTP source URL is invalid: {error}"),
                )
            })?;
        }
        run_sftp_download(&ctx, &task, &config, control, &self.known_hosts).await
    }
}

async fn run_sftp_download(
    ctx: &EngineContext,
    task: &PreparedTask,
    config: &SftpTaskConfig,
    control: TaskControl,
    known_hosts: &std::path::Path,
) -> Result<EngineExit> {
    tokio::fs::create_dir_all(&task.task.save_dir).await?;
    let file_name = task
        .task
        .file_name
        .clone()
        .or_else(|| inferred_file_name(&config.url))
        .unwrap_or_else(|| "download".to_string());
    let output = task.task.save_dir.join(sanitize_file_name(&file_name));
    let part = part_path(&output);
    let remote_path = remote_path(&config.url)?;
    let username = task
        .credentials
        .username
        .as_deref()
        .or(config.username.as_deref())
        .ok_or_else(|| {
            FluxionError::new(FluxionErrorKind::InvalidConfig, "SFTP username is required")
        })?;
    let addr = sftp_addr(&config.url)?;
    let handler = SftpClient {
        host: addr.0.clone(),
        port: addr.1,
        known_hosts: known_hosts.into(),
    };
    let mut session = control
        .network_io(async {
            client::connect(Arc::new(client::Config::default()), addr, handler)
                .await
                .map_err(sftp_error)
        })
        .await?;
    let authenticated = control
        .network_io(authenticate_sftp(&mut session, username, task, config))
        .await?;
    if !authenticated {
        return Err(FluxionError::new(
            FluxionErrorKind::Unauthorized,
            "SFTP authentication failed",
        ));
    }
    let channel = control
        .network_io(async { session.channel_open_session().await.map_err(sftp_error) })
        .await?;
    control
        .network_io(async {
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(sftp_error)
        })
        .await?;
    let sftp = control
        .network_io(async {
            SftpSession::new(channel.into_stream())
                .await
                .map_err(sftp_client_error)
        })
        .await?;
    let total = control
        .network_io(async {
            sftp.metadata(remote_path.clone())
                .await
                .map_err(sftp_client_error)
        })
        .await?
        .size;
    let existing = existing_len(&part).await?;
    if let Some(total) = total
        && existing > total
    {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            format!("partial SFTP file is larger than remote file: {existing} > {total}"),
        ));
    }
    let mut remote = control
        .network_io(async { sftp.open(remote_path).await.map_err(sftp_client_error) })
        .await?;
    if existing > 0 {
        control
            .network_io(async {
                remote
                    .seek(std::io::SeekFrom::Start(existing))
                    .await
                    .map_err(FluxionError::from)
            })
            .await?;
    }
    let mut local = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .await?;
    let mut downloaded = existing;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        if control.is_cancelled() {
            local.flush().await?;
            return Err(FluxionError::cancelled());
        }
        let read = match control
            .network_io(async { remote.read(&mut buffer).await.map_err(FluxionError::from) })
            .await
        {
            Ok(read) => read,
            Err(error) => {
                local.flush().await?;
                return Err(error);
            }
        };
        if read == 0 {
            break;
        }
        control.acquire_download(read as u64).await;
        if control.is_cancelled() {
            local.flush().await?;
            return Err(FluxionError::cancelled());
        }
        local.write_all(&buffer[..read]).await?;
        downloaded += read as u64;
        throttled_progress(ctx, task.task.id, downloaded, total, false).await?;
    }
    local.flush().await?;
    control
        .network_io(async { remote.shutdown().await.map_err(FluxionError::from) })
        .await?;
    control
        .network_io(async { sftp.close().await.map_err(sftp_client_error) })
        .await?;
    throttled_progress(ctx, task.task.id, downloaded, total, true).await?;
    validate_size(&part, total).await?;
    tokio::fs::rename(part, &output).await?;
    Ok(EngineExit::Completed {
        file_path: Some(output),
    })
}

async fn authenticate_sftp(
    session: &mut client::Handle<SftpClient>,
    username: &str,
    task: &PreparedTask,
    config: &SftpTaskConfig,
) -> Result<bool> {
    if let Some(private_key_path) = &config.private_key_path {
        let passphrase = task.credentials.private_key_passphrase.as_deref();
        let key_path = expand_home_path(private_key_path);
        let key = load_secret_key(&key_path, passphrase).map_err(sftp_key_error)?;
        let hash_alg = session
            .best_supported_rsa_hash()
            .await
            .map_err(sftp_error)?
            .flatten();
        let authenticated = session
            .authenticate_publickey(
                username,
                PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
            )
            .await
            .map(|result| result.success())
            .map_err(sftp_error)?;
        if authenticated || task.credentials.password.is_none() {
            return Ok(authenticated);
        }
    }

    let password = task.credentials.password.as_deref().ok_or_else(|| {
        FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "SFTP password is required when private key authentication is not configured",
        )
    })?;
    session
        .authenticate_password(username, password)
        .await
        .map(|result| result.success())
        .map_err(sftp_error)
}

fn expand_home_path(path: &std::path::Path) -> PathBuf {
    let Some(value) = path.to_str() else {
        return path.to_path_buf();
    };
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path.to_path_buf()
}

struct SftpClient {
    host: String,
    port: u16,
    known_hosts: PathBuf,
}

impl client::Handler for SftpClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fingerprint = server_public_key
            .fingerprint(russh::keys::HashAlg::Sha256)
            .to_string();
        Ok(verify_host_key_at(
            &self.known_hosts,
            &self.host,
            self.port,
            &fingerprint,
        ))
    }
}

/// Trust-on-first-use host key verification backed by a known_hosts file in
/// the Fluxion data directory. The first connection records the server key
/// fingerprint; later connections must present the same key.
static HOST_KEY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn verify_host_key_at(path: &std::path::Path, host: &str, port: u16, fingerprint: &str) -> bool {
    use std::io::Write;
    // Serialize read/modify/write so concurrent connections in this process do
    // not discard another host's pin or accept two different first-use keys.
    let Ok(_guard) = HOST_KEY_LOCK.lock() else {
        return false;
    };
    use std::os::unix::fs::OpenOptionsExt;
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let Ok(lock_file) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(PathBuf::from(lock_path))
    else {
        return false;
    };
    if lock_file.lock().is_err() {
        return false;
    }
    let entry_key = format!("{host}:{port}");
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return false,
    };
    let mut found = false;
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let (Some(key), Some(stored)) = (parts.next(), parts.next()) else {
            continue;
        };
        if key == entry_key {
            if stored != fingerprint {
                tracing::error!(
                    host = entry_key,
                    "SFTP host key mismatch; refusing connection"
                );
                return false;
            }
            found = true;
        }
    }
    if found {
        return true;
    }
    let mut updated = contents;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&format!("{entry_key} {fingerprint}\n"));
    // NamedTempFile uses private permissions and atomically replaces the pin
    // database only after the complete contents have been flushed.
    let saved = (|| -> std::io::Result<()> {
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(updated.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|e| e.error)?;
        Ok(())
    })();
    if saved.is_err() {
        tracing::warn!("SFTP host key could not be saved; refusing connection");
        return false;
    }
    true
}

fn known_hosts_path() -> PathBuf {
    std::env::var_os("FLUXION_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".fluxion")
        })
        .join("known_hosts")
}

fn part_path(path: &std::path::Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".fluxionpart");
    PathBuf::from(os)
}

fn sftp_addr(url: &Url) -> Result<(String, u16)> {
    let host = url.host_str().ok_or_else(|| {
        FluxionError::new(FluxionErrorKind::InvalidConfig, "SFTP host is required")
    })?;
    Ok((host.to_string(), url.port_or_known_default().unwrap_or(22)))
}

fn remote_path(url: &Url) -> Result<String> {
    let path = url.path();
    if path.is_empty() || path == "/" {
        return Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "SFTP URL must point to a file path",
        ));
    }
    percent_decode(path)
}

fn percent_decode(input: &str) -> Result<String> {
    percent_encoding::percent_decode_str(input)
        .decode_utf8()
        .map(|value| value.into_owned())
        .map_err(|_| {
            FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "SFTP URL path is not valid UTF-8 after percent-decoding",
            )
        })
}

fn inferred_file_name(url: &Url) -> Option<String> {
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|value| !value.is_empty())
        .and_then(|value| percent_decode(value).ok())
}

fn sanitize_file_name(input: &str) -> String {
    fluxion_core::sanitize_file_name(input)
}

async fn existing_len(path: &std::path::Path) -> Result<u64> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

async fn validate_size(path: &std::path::Path, expected: Option<u64>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = tokio::fs::metadata(path).await?.len();
    if actual != expected {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            format!("downloaded SFTP file size mismatch: expected {expected}, got {actual}"),
        ));
    }
    Ok(())
}

async fn throttled_progress(
    ctx: &EngineContext,
    task_id: fluxion_core::TaskId,
    downloaded: u64,
    total: Option<u64>,
    force: bool,
) -> Result<()> {
    if force || downloaded % (1024 * 1024) < BUFFER_SIZE as u64 {
        ctx.progress(task_id, downloaded, 0, total).await?;
    }
    Ok(())
}

fn sftp_error(error: russh::Error) -> FluxionError {
    if matches!(error, russh::Error::UnknownKey) {
        return FluxionError::new(
            FluxionErrorKind::Unauthorized,
            "SFTP server host key does not match the fingerprint recorded in known_hosts \
             (possible man-in-the-middle attack)",
        );
    }
    FluxionError::new(FluxionErrorKind::Network, error.to_string())
}

fn sftp_key_error(error: russh::keys::Error) -> FluxionError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("permission") {
        FluxionErrorKind::PermissionDenied
    } else if lower.contains("no such") || lower.contains("not found") {
        FluxionErrorKind::NotFound
    } else if lower.contains("decrypt")
        || lower.contains("password")
        || lower.contains("passphrase")
    {
        FluxionErrorKind::Unauthorized
    } else {
        FluxionErrorKind::InvalidConfig
    };
    FluxionError::new(kind, format!("failed to load SFTP private key: {message}"))
}

fn sftp_client_error(error: russh_sftp::client::error::Error) -> FluxionError {
    let kind = match &error {
        russh_sftp::client::error::Error::Status(status)
            if status
                .error_message
                .to_ascii_lowercase()
                .contains("permission") =>
        {
            FluxionErrorKind::PermissionDenied
        }
        russh_sftp::client::error::Error::Status(status)
            if status
                .error_message
                .to_ascii_lowercase()
                .contains("no such") =>
        {
            FluxionErrorKind::NotFound
        }
        _ => FluxionErrorKind::Network,
    };
    FluxionError::new(kind, error.to_string())
}

#[cfg(test)]
mod host_key_tests {
    use super::verify_host_key_at;
    #[test]
    fn first_use_pins_key_and_changed_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        assert!(verify_host_key_at(
            &path,
            "example.test",
            22,
            "SHA256:first"
        ));
        assert!(verify_host_key_at(
            &path,
            "example.test",
            22,
            "SHA256:first"
        ));
        assert!(!verify_host_key_at(
            &path,
            "example.test",
            22,
            "SHA256:changed"
        ));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "example.test:22 SHA256:first\n"
        );
    }
    #[test]
    fn unreadable_or_unwritable_pin_store_never_establishes_trust() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!verify_host_key_at(
            dir.path(),
            "example.test",
            22,
            "SHA256:first"
        ));
        let file = dir.path().join("not-a-directory");
        std::fs::write(&file, b"fixture").unwrap();
        assert!(!verify_host_key_at(
            &file.join("known_hosts"),
            "example.test",
            22,
            "SHA256:first"
        ));
    }
    #[test]
    fn concurrent_first_use_preserves_every_pin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        std::thread::scope(|scope| {
            for n in 0..16 {
                let path = &path;
                scope.spawn(move || {
                    assert!(verify_host_key_at(
                        path,
                        &format!("host{n}"),
                        22,
                        "SHA256:key"
                    ))
                });
            }
        });
        assert_eq!(std::fs::read_to_string(path).unwrap().lines().count(), 16);
    }
}
