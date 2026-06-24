use std::path::PathBuf;

use anyhow::{Context, Result};
use fluxion_core::{CreateTaskInput, SettingsSnapshot, TaskFilter, TaskId};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcParams {
    Empty,
    TaskId { task_id: TaskId },
    DeleteTask { task_id: TaskId, delete_files: bool },
    CreateTask(Box<CreateTaskInput>),
    TaskFilter(TaskFilter),
    Settings(Box<SettingsSnapshot>),
}

pub fn default_socket_path() -> PathBuf {
    std::env::var_os("FLUXION_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/fluxiond.sock"))
}

pub fn default_data_dir() -> PathBuf {
    std::env::var_os("FLUXION_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".fluxion")
        })
}

pub fn default_pid_path() -> PathBuf {
    std::env::var_os("FLUXION_PID")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_data_dir().join("fluxiond.pid"))
}

pub async fn request<T: for<'de> Deserialize<'de>>(
    socket_path: PathBuf,
    method: &str,
    params: IpcParams,
) -> Result<T> {
    let mut stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("connect {}", socket_path.display()))?;
    let request = IpcRequest {
        id: 1,
        method: method.to_string(),
        params: serde_json::to_value(params)?,
    };
    stream
        .write_all(format!("{}\n", serde_json::to_string(&request)?).as_bytes())
        .await?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let response: IpcResponse = serde_json::from_str(&line)?;
    if let Some(error) = response.error {
        anyhow::bail!(error);
    }
    serde_json::from_value(response.result.unwrap_or(serde_json::Value::Null)).map_err(Into::into)
}

pub async fn listen(socket_path: PathBuf) -> Result<UnixListener> {
    if socket_path.exists() {
        match UnixStream::connect(&socket_path).await {
            Ok(_) => anyhow::bail!(
                "fluxiond socket is already active at {}",
                socket_path.display()
            ),
            Err(_) => {
                let _ = tokio::fs::remove_file(&socket_path).await;
            }
        }
    }
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    UnixListener::bind(socket_path).map_err(Into::into)
}
