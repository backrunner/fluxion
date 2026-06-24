use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use fluxion_core::{CreateTaskInput, TaskFilter, TaskId};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::{Notify, broadcast},
};
use tracing_subscriber::EnvFilter;

use crate::{
    ipc::{IpcParams, IpcRequest, IpcResponse, default_pid_path, default_socket_path, listen},
    runtime,
};

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let core = runtime::build_core(runtime::default_data_dir()).await?;
    let socket = default_socket_path();
    let listener = listen(socket.clone()).await?;
    let pid_path = default_pid_path();
    write_pid_file(&pid_path)?;
    let shutdown = ShutdownSignal::default();
    println!("fluxiond listening on {}", socket.display());
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let core = core.clone();
                let shutdown = shutdown.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_client(core, stream, shutdown).await {
                        tracing::warn!(%error, "IPC client failed");
                    }
                });
            }
            () = shutdown.wait() => {
                break;
            }
        }
    }
    cleanup_runtime_files(&pid_path, &socket).await;
    Ok(())
}

#[derive(Clone, Default)]
struct ShutdownSignal {
    requested: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl ShutdownSignal {
    fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            if self.requested.load(Ordering::SeqCst) {
                return;
            }
            self.notify.notified().await;
        }
    }
}

fn write_pid_file(pid_path: &std::path::Path) -> Result<()> {
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(pid_path, std::process::id().to_string())?;
    Ok(())
}

async fn cleanup_runtime_files(pid_path: &std::path::Path, socket_path: &std::path::Path) {
    let _ = tokio::fs::remove_file(pid_path).await;
    let _ = tokio::fs::remove_file(socket_path).await;
}

async fn handle_client(
    core: Arc<fluxion_core::FluxionCore>,
    stream: UnixStream,
    shutdown: ShutdownSignal,
) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    while reader.read_line(&mut line).await? > 0 {
        let request: IpcRequest = serde_json::from_str(&line)?;
        if request.method == "events.subscribe" {
            stream_events(core.clone(), request.id, &mut write).await?;
            break;
        }
        let should_shutdown = request.method == "daemon.stop";
        let response = dispatch(core.clone(), request, shutdown.clone()).await;
        write
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        if should_shutdown {
            break;
        }
        line.clear();
    }
    Ok(())
}

async fn dispatch(
    core: Arc<fluxion_core::FluxionCore>,
    request: IpcRequest,
    shutdown: ShutdownSignal,
) -> IpcResponse {
    let result = match request.method.as_str() {
        "task.create" => match parse_params::<CreateTaskInput>(request.params) {
            Ok(input) => core.create_task(input).await.map(json).map_err(to_string),
            Err(error) => Err(error),
        },
        "task.start" => match parse_params::<IpcParams>(request.params) {
            Ok(IpcParams::TaskId { task_id }) => {
                core.start_task(task_id).await.map(json).map_err(to_string)
            }
            Ok(_) => Err("invalid params".to_string()),
            Err(error) => Err(error),
        },
        "task.pause" => {
            task_id_call(&core, request.params, |core, task_id| async move {
                core.pause_task(task_id).await
            })
            .await
        }
        "task.stop" => {
            task_id_call(&core, request.params, |core, task_id| async move {
                core.stop_task(task_id).await
            })
            .await
        }
        "task.delete" => {
            let params: Result<IpcParams, _> = serde_json::from_value(request.params);
            match params {
                Ok(IpcParams::DeleteTask {
                    task_id,
                    delete_files,
                }) => core
                    .delete_task(task_id, delete_files)
                    .await
                    .map(json)
                    .map_err(to_string),
                Ok(_) => Err("invalid params".to_string()),
                Err(error) => Err(error.to_string()),
            }
        }
        "task.list" => {
            let params: Result<IpcParams, _> = serde_json::from_value(request.params);
            match params {
                Ok(IpcParams::TaskFilter(filter)) => {
                    core.list_tasks(filter).await.map(json).map_err(to_string)
                }
                Ok(IpcParams::Empty) => core
                    .list_tasks(TaskFilter::default())
                    .await
                    .map(json)
                    .map_err(to_string),
                Ok(_) => Err("invalid params".to_string()),
                Err(error) => Err(error.to_string()),
            }
        }
        "task.get" => {
            let params: Result<IpcParams, _> = serde_json::from_value(request.params);
            match params {
                Ok(IpcParams::TaskId { task_id }) => core
                    .get_task(task_id)
                    .await
                    .map(|task| task.map(|detail| detail.redacted()))
                    .map(json)
                    .map_err(to_string),
                Ok(_) => Err("invalid params".to_string()),
                Err(error) => Err(error.to_string()),
            }
        }
        "settings.get" => core.get_settings().await.map(json).map_err(to_string),
        "settings.update" => {
            let params: Result<IpcParams, _> = serde_json::from_value(request.params);
            match params {
                Ok(IpcParams::Settings(settings)) => core
                    .update_settings(*settings)
                    .await
                    .map(json)
                    .map_err(to_string),
                Ok(_) => Err("invalid params".to_string()),
                Err(error) => Err(error.to_string()),
            }
        }
        "daemon.status" => Ok(json("running")),
        "daemon.stop" => {
            shutdown.request();
            Ok(json("stopping"))
        }
        _ => Err(format!("unknown method {}", request.method)),
    };

    match result {
        Ok(value) => IpcResponse {
            id: request.id,
            result: Some(value),
            error: None,
        },
        Err(error) => IpcResponse {
            id: request.id,
            result: None,
            error: Some(error.to_string()),
        },
    }
}

async fn task_id_call<F, Fut>(
    core: &Arc<fluxion_core::FluxionCore>,
    params: serde_json::Value,
    f: F,
) -> Result<serde_json::Value, String>
where
    F: FnOnce(Arc<fluxion_core::FluxionCore>, TaskId) -> Fut,
    Fut: std::future::Future<Output = fluxion_core::Result<()>>,
{
    let params: IpcParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    match params {
        IpcParams::TaskId { task_id } => f(core.clone(), task_id)
            .await
            .map(json)
            .map_err(|error| error.to_string()),
        _ => Err("invalid params".to_string()),
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn json<T: serde::Serialize>(value: T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

async fn stream_events(
    core: Arc<fluxion_core::FluxionCore>,
    id: u64,
    write: &mut tokio::net::unix::OwnedWriteHalf,
) -> Result<()> {
    let mut receiver = core.subscribe_events();
    loop {
        match receiver.recv().await {
            Ok(event) => {
                let response = IpcResponse {
                    id,
                    result: Some(json(event)),
                    error: None,
                };
                write
                    .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                    .await?;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    Ok(())
}
