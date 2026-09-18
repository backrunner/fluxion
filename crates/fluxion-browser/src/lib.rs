//! Browser/App boundary. No protocol engines or second Core/database owner.
use anyhow::{Result, ensure};
use fluxion_core::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
use url::Url;

pub const HOST_NAME: &str = "top.backrunner.fluxion";
pub const EXTENSION_ID: &str = "mkfokhcdpnpilifmdeecgkghkpicikaa";
pub const MAX_MESSAGE: usize = 128 * 1024;

#[derive(Serialize)]
pub struct Request {
    pub version: u8,
    #[serde(flatten)]
    pub command: Command,
}

// Strict tagged wire decoding avoids flatten/deny_unknown_fields ambiguities.
impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
        enum Wire {
            Ping { version: u8 },
            AddDownload { version: u8, download: Download },
        }
        let (version, command) = match Wire::deserialize(deserializer)? {
            Wire::Ping { version } => (version, Command::Ping),
            Wire::AddDownload { version, download } => (version, Command::AddDownload { download }),
        };
        Ok(Self { version, command })
    }
}

#[derive(Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Command {
    Ping,
    AddDownload { download: Download },
}

// Deliberately no Debug implementation: URLs and headers contain secrets.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Download {
    pub url: String,
    pub filename: Option<String>,
    pub headers: Vec<Header>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    pub ok: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
}

impl Response {
    pub fn status(status: &str) -> Self {
        Self {
            ok: true,
            status: status.into(),
            task_id: None,
        }
    }
    pub fn error(status: &str) -> Self {
        Self {
            ok: false,
            status: status.into(),
            task_id: None,
        }
    }
    pub fn task(status: &str, id: TaskId) -> Self {
        Self {
            task_id: Some(id),
            ..Self::status(status)
        }
    }
}

pub fn allowed_origin(origin: &str) -> bool {
    origin == format!("chrome-extension://{EXTENSION_ID}/")
}

pub fn bridge_dir() -> PathBuf {
    std::env::var_os("FLUXION_BROWSER_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".fluxion/browser")
        })
}

pub fn socket_path() -> PathBuf {
    bridge_dir().join("bridge.sock")
}

pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    // Native Messaging uses native-endian u32 lengths (macOS is little-endian).
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix).await?;
    let size = u32::from_ne_bytes(prefix) as usize;
    ensure!(size > 0 && size <= MAX_MESSAGE, "Invalid message length");
    let mut body = vec![0; size];
    reader.read_exact(&mut body).await?;
    Ok(body)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, body: &[u8]) -> Result<()> {
    ensure!(
        !body.is_empty() && body.len() <= MAX_MESSAGE,
        "Invalid message length"
    );
    writer.write_all(&(body.len() as u32).to_ne_bytes()).await?;
    writer.write_all(body).await?;
    writer.flush().await?;
    Ok(())
}

fn allowed_header(name: &str) -> bool {
    matches!(
        name,
        "cookie"
            | "authorization"
            | "referer"
            | "origin"
            | "user-agent"
            | "accept"
            | "accept-language"
    ) || (name.starts_with("x-")
        && !matches!(
            name,
            "x-forwarded-for" | "x-forwarded-host" | "x-forwarded-proto"
        ))
}

impl Download {
    pub fn validate(self) -> Result<Self> {
        let mut url = Url::parse(&self.url).map_err(|_| anyhow::anyhow!("Invalid download URL"))?;
        ensure!(
            matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
            "Only HTTP downloads are supported"
        );
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "URL credentials are not supported"
        );
        ensure!(
            self.url.len() <= 16 * 1024 && self.headers.len() <= 64,
            "Download metadata is too large"
        );
        let filename = self.filename.filter(|s| !s.is_empty()).unwrap_or_else(|| {
            percent_decode_lossy(
                url.path_segments()
                    .and_then(|mut s| s.next_back())
                    .unwrap_or("download"),
            )
        });
        ensure!(filename.len() <= 1024, "Filename is too long");
        // Browser paths are suggestions only. Never inherit a browser's absolute directory.
        let filename = filename.rsplit(['/', '\\']).next().unwrap_or("download");
        let filename = sanitize_file_name(filename);
        ensure!(filename.len() <= 240, "Filename is too long");
        ensure!(!filename.chars().any(char::is_control), "Invalid filename");
        let mut headers = Vec::new();
        for header in self.headers {
            let name = header.name.to_ascii_lowercase();
            ensure!(
                name.len() <= 128 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                "Invalid header name"
            );
            ensure!(allowed_header(&name), "Unsupported request header");
            ensure!(
                header.value.len() <= 16 * 1024
                    && !header.value.chars().any(|c| c.is_control() && c != '\t'),
                "Invalid header value"
            );
            ensure!(
                !headers.iter().any(|h: &Header| h.name == name),
                "Duplicate request header"
            );
            headers.push(Header {
                name,
                value: header.value,
            });
        }
        url.set_fragment(None);
        Ok(Self {
            url: url.to_string(),
            filename: Some(filename),
            headers,
        })
    }
}

/// An in-memory request for the ordinary Add Task form. The receiver owns the
/// decision; there is no Core, database or SecretStore access in this bridge.
pub struct IncomingDownload {
    pub download: Download,
    pub reply: tokio::sync::oneshot::Sender<Response>,
}

async fn handle(
    request: Request,
    drafts: &tokio::sync::mpsc::Sender<IncomingDownload>,
) -> Response {
    if request.version != 1 {
        return Response::error("unsupported_version");
    }
    match request.command {
        Command::Ping => Response::status("connected"),
        Command::AddDownload { download } => {
            let download = match download.validate() {
                Ok(download) => download,
                Err(_) => return Response::error("invalid_download"),
            };
            let (reply, decision) = tokio::sync::oneshot::channel();
            if drafts
                .try_send(IncomingDownload { download, reply })
                .is_err()
            {
                return Response::error("app_busy");
            }
            // Wait for the user's normal form confirmation, not a storage side effect.
            decision
                .await
                .unwrap_or_else(|_| Response::error("cancelled"))
        }
    }
}

pub async fn bind(directory: &Path) -> Result<UnixListener> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    tokio::fs::create_dir_all(directory).await?;
    let meta = tokio::fs::symlink_metadata(directory).await?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "Invalid browser bridge directory"
    );
    tokio::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).await?;
    let path = directory.join("bridge.sock");
    if tokio::fs::symlink_metadata(&path).await.is_ok() {
        ensure!(
            UnixStream::connect(&path).await.is_err(),
            "Browser bridge is already running"
        );
        let socket = tokio::fs::symlink_metadata(&path).await?;
        ensure!(socket.uid() == meta.uid(), "Invalid socket owner");
        tokio::fs::remove_file(&path).await?;
    }
    let listener = UnixListener::bind(&path)?;
    tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(listener)
}

pub async fn serve(listener: UnixListener, drafts: tokio::sync::mpsc::Sender<IncomingDownload>) {
    // A pending form must not block ping or other connections. JoinSet owns and
    // cancels all workers on shutdown; both the queue and connections are bounded.
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _ = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept(), if connections.len() < 8 => {
                let Ok((stream, _)) = accepted else { break; };
                let drafts = drafts.clone();
                connections.spawn(serve_connection(stream, drafts));
            }
        }
    }
}

async fn serve_connection(
    mut stream: UnixStream,
    drafts: tokio::sync::mpsc::Sender<IncomingDownload>,
) {
    let body = match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        read_frame(&mut stream),
    )
    .await
    {
        Ok(Ok(body)) => body,
        _ => return,
    };
    let response = match serde_json::from_slice::<Request>(&body) {
        Ok(request) => tokio::select! {
            response = handle(request, &drafts) => response,
            // No further input belongs to this one-message connection. On host
            // disconnect, drop the receiver so a stale form cannot be submitted.
            _ = stream.read_u8() => return,
        },
        Err(_) => Response::error("invalid_message"),
    };
    if let Ok(body) = serde_json::to_vec(&response) {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            write_frame(&mut stream, &body),
        )
        .await;
    }
}

/// Register only this extension, using an installed/bundled absolute helper path.
pub fn register_hosts(host: &Path, home: &Path) -> Result<()> {
    let host = host.canonicalize()?;
    let manifest = serde_json::json!({
        "name": HOST_NAME, "description": "Fluxion browser downloads", "path": host,
        "type": "stdio", "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")]
    });
    for browser in [
        "Google/Chrome",
        "Microsoft Edge",
        "BraveSoftware/Brave-Browser",
        "Chromium",
    ] {
        let directory = home
            .join("Library/Application Support")
            .join(browser)
            .join("NativeMessagingHosts");
        std::fs::create_dir_all(&directory)?;
        let target = directory.join(format!("{HOST_NAME}.json"));
        let temp = target.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(&manifest)?)?;
        std::fs::rename(temp, target)?;
    }
    Ok(())
}
