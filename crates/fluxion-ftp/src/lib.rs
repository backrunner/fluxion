use std::path::PathBuf;

use async_trait::async_trait;
use fluxion_core::{
    DownloadEngine, DownloadKind, EngineContext, EngineExit, FluxionError, FluxionErrorKind,
    FtpTaskConfig, PreparedTask, Result, TaskControl, TaskKind,
};
use suppaftp::{Mode, tokio::AsyncFtpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

const BUFFER_SIZE: usize = 128 * 1024;

pub struct FtpEngine;

impl FtpEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FtpEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DownloadEngine for FtpEngine {
    fn kind(&self) -> DownloadKind {
        DownloadKind::Ftp
    }

    async fn prepare(&self, ctx: EngineContext, mut task: PreparedTask) -> Result<PreparedTask> {
        let TaskKind::Ftp(config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an FTP task",
            ));
        };
        if task.task.file_name.is_none() {
            task.task.file_name = inferred_file_name(&config.url);
            ctx.storage.update_task(task.task.clone()).await?;
        }
        Ok(task)
    }

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit> {
        let TaskKind::Ftp(config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an FTP task",
            ));
        };
        run_ftp_download(&ctx, &task, config, control).await
    }
}

async fn run_ftp_download(
    ctx: &EngineContext,
    task: &PreparedTask,
    config: &FtpTaskConfig,
    control: TaskControl,
) -> Result<EngineExit> {
    if config.ftps {
        return Err(FluxionError::new(
            FluxionErrorKind::Unsupported,
            "FTPS requires enabling a suppaftp async TLS feature; plain FTP is supported",
        ));
    }
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
    let addr = ftp_addr(&config.url)?;
    let username = task
        .credentials
        .username
        .as_deref()
        .or(config.username.as_deref())
        .unwrap_or("anonymous");
    let password = task.credentials.password.as_deref().unwrap_or("anonymous@");

    let mut ftp = AsyncFtpStream::connect(addr).await.map_err(ftp_error)?;
    if !config.passive {
        ftp.set_mode(Mode::Active);
    }
    ftp.login(username, password).await.map_err(ftp_error)?;
    let total = ftp.size(&remote_path).await.ok().map(|value| value as u64);
    let existing = existing_len(&part).await?;
    if let Some(total) = total
        && existing > total
    {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            format!("partial FTP file is larger than remote file: {existing} > {total}"),
        ));
    }
    if existing > 0 {
        ftp.resume_transfer(existing as usize)
            .await
            .map_err(ftp_error)?;
    }
    let mut stream = ftp.retr_as_stream(&remote_path).await.map_err(ftp_error)?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .await?;
    let mut downloaded = existing;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        if control.is_cancelled() {
            file.flush().await?;
            drop(stream);
            return Err(FluxionError::cancelled());
        }
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        control.acquire_download(read as u64).await;
        file.write_all(&buffer[..read]).await?;
        downloaded += read as u64;
        throttled_progress(ctx, task.task.id, downloaded, total, false).await?;
    }
    file.flush().await?;
    ftp.finalize_retr_stream(stream).await.map_err(ftp_error)?;
    throttled_progress(ctx, task.task.id, downloaded, total, true).await?;
    validate_size(&part, total).await?;
    tokio::fs::rename(part, &output).await?;
    Ok(EngineExit::Completed {
        file_path: Some(output),
    })
}

fn part_path(path: &std::path::Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".fluxionpart");
    PathBuf::from(os)
}

fn ftp_addr(url: &Url) -> Result<String> {
    let host = url.host_str().ok_or_else(|| {
        FluxionError::new(FluxionErrorKind::InvalidConfig, "FTP host is required")
    })?;
    let port = url.port_or_known_default().unwrap_or(21);
    Ok(format!("{host}:{port}"))
}

fn remote_path(url: &Url) -> Result<String> {
    let path = url.path();
    if path.is_empty() || path == "/" {
        return Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "FTP URL must point to a file path",
        ));
    }
    Ok(path.to_string())
}

fn inferred_file_name(url: &Url) -> Option<String> {
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn sanitize_file_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ => ch,
        })
        .collect()
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
            format!("downloaded FTP file size mismatch: expected {expected}, got {actual}"),
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

fn ftp_error(error: suppaftp::FtpError) -> FluxionError {
    let kind = match error {
        suppaftp::FtpError::UnexpectedResponse(ref response) => match response.status.code() {
            401 | 430 | 530 => FluxionErrorKind::Unauthorized,
            450 | 550 => FluxionErrorKind::NotFound,
            _ => FluxionErrorKind::Network,
        },
        suppaftp::FtpError::ConnectionError(_) => FluxionErrorKind::Network,
        _ => FluxionErrorKind::Unknown,
    };
    FluxionError::new(kind, error.to_string())
}
