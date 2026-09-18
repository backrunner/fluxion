//! The native App boundary. Network, storage and file operations run on Tokio,
//! while GPUI only receives redacted, owned snapshots and bounded events.
use fluxion_core::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Pause,
    Stop,
    Trash,
    Restore,
    Delete(bool),
    Open,
    Reveal,
}

pub enum Command {
    Refresh,
    CheckUpdate(crate::updater::Channel, bool),
    DownloadUpdate(crate::updater::Release, crate::updater::Channel),
    InstallUpdate(crate::updater::PreparedUpdate),
    Detail(TaskId),
    Bt(TaskId),
    Create(CreateTaskInput),
    Act(Vec<TaskId>, Action),
    Settings(SettingsSnapshot),
    Limits(TaskId, TaskRateLimit),
    Preview(u64, String),
    Preferences(crate::model::Preferences),
}

pub enum Message {
    Snapshot(Vec<TaskSummary>, SettingsSnapshot),
    Event(CoreEvent),
    Detail(TaskId, Option<TaskDetail>),
    Bt(TaskId, Option<BtStateSnapshot>),
    Created(TaskId),
    BrowserDownload(fluxion_browser::IncomingDownload),
    Acted(TaskId, Action),
    Saved,
    Preview(u64, std::result::Result<MagnetPreview, String>),
    Error(String),
    Finished,
    UpdateChecked(
        crate::updater::Channel,
        bool,
        std::result::Result<Option<crate::updater::Release>, String>,
    ),
    UpdateProgress(u64, Option<u64>),
    UpdateVerifying,
    UpdateReady(crate::updater::PreparedUpdate),
    UpdateError(String),
    Restart,
}

pub struct Backend {
    pub commands: async_channel::Sender<Command>,
    pub messages: async_channel::Receiver<Message>,
    pub stopped: async_channel::Receiver<()>,
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("FLUXION_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // Preserve the production Tauri app's existing database and Keychain service.
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Library/Application Support/top.backrunner.fluxion/core")
        })
}

impl Backend {
    pub fn start(runtime: &tokio::runtime::Runtime, data: PathBuf) -> Self {
        let (commands, rx) = async_channel::bounded(32);
        let (tx, messages) = async_channel::bounded(512);
        let (done, stopped) = async_channel::bounded(1);
        let serve = async move {
            let core = match fluxion_runtime::build_core(data.clone()).await {
                Ok(core) => core,
                Err(_) => {
                    let _ = tx.send(Message::Error("Unable to open the download database. Check permissions and restart Fluxion.".into())).await;
                    return;
                }
            };
            let mut events = core.subscribe_events();
            if let Err(error) = snapshot(&core, &tx).await {
                let _ = tx.send(Message::Error(error.to_string())).await;
            }
            let event_tx = tx.clone();
            let event_core = core.clone();
            let forward = tokio::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            if event_tx.send(Message::Event(event)).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            if let Err(error) = snapshot(&event_core, &event_tx).await {
                                let _ = event_tx.send(Message::Error(error.to_string())).await;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            // Preview/test databases do not claim the production browser bridge.
            let browser_bridge = if data == data_dir()
                && (std::env::var_os("FLUXION_DATA_DIR").is_none()
                    || std::env::var_os("FLUXION_BROWSER_DIR").is_some())
            {
                match fluxion_browser::bind(&fluxion_browser::bridge_dir()).await {
                    Ok(listener) => {
                        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                        if std::env::var_os("FLUXION_DATA_DIR").is_none()
                            && let Ok(executable) = std::env::current_exe()
                            && let Some(parent) = executable.parent()
                        {
                            let host = parent.join("fluxion-browser-host");
                            if host.is_file() {
                                let registration_home = home.clone();
                                // Registration is small filesystem work, kept off GPUI.
                                let _ = tokio::task::spawn_blocking(move || {
                                    fluxion_browser::register_hosts(&host, &registration_home)
                                })
                                .await;
                            }
                        }
                        let (drafts, mut incoming) = tokio::sync::mpsc::channel(1);
                        let browser_tx = tx.clone();
                        Some(tokio::spawn(async move {
                            let server = fluxion_browser::serve(listener, drafts);
                            let forward = async move {
                                while let Some(draft) = incoming.recv().await {
                                    if browser_tx
                                        .send(Message::BrowserDownload(draft))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            };
                            tokio::select! { _ = server => {}, _ = forward => {} }
                        }))
                    }
                    Err(_) => {
                        let _ = tx.send(Message::Error("Browser integration is unavailable. Close other Fluxion instances and restart.".into())).await;
                        None
                    }
                }
            } else {
                None
            };
            let mut preview_task: Option<tokio::task::JoinHandle<()>> = None;
            let mut update_task: Option<tokio::task::JoinHandle<()>> = None;
            let mut check_task: Option<tokio::task::JoinHandle<()>> = None;
            while let Ok(command) = rx.recv().await {
                if let Command::CheckUpdate(channel, manual) = command {
                    if update_task.as_ref().is_some_and(|t| !t.is_finished()) {
                        continue;
                    }
                    if let Some(task) = check_task.take() {
                        task.abort();
                    }
                    let tx = tx.clone();
                    check_task = Some(tokio::spawn(async move {
                        let result = crate::updater::check(channel).await.map_err(|_| {
                            "Unable to check for updates. Please try again later.".into()
                        });
                        let _ = tx
                            .send(Message::UpdateChecked(channel, manual, result))
                            .await;
                    }));
                    continue;
                }
                if matches!(
                    command,
                    Command::DownloadUpdate(..) | Command::InstallUpdate(_)
                ) {
                    if update_task.as_ref().is_some_and(|t| !t.is_finished()) {
                        continue;
                    }
                    if let Some(task) = check_task.take() {
                        task.abort();
                    }
                    let tx = tx.clone();
                    update_task = Some(tokio::spawn(async move {
                        match command {
                            Command::DownloadUpdate(release, channel) => {
                                match crate::updater::prepare(release, channel, tx.clone()).await {
                                    Ok(prepared) => {
                                        let _ = tx.send(Message::UpdateReady(prepared)).await;
                                    }
                                    Err(error) => {
                                        let _ =
                                            tx.send(Message::UpdateError(error.to_string())).await;
                                    }
                                }
                            }
                            Command::InstallUpdate(prepared) => {
                                match crate::updater::install(prepared).await {
                                    Ok(()) => {
                                        let _ = tx.send(Message::Restart).await;
                                    }
                                    Err(error) => {
                                        let _ =
                                            tx.send(Message::UpdateError(error.to_string())).await;
                                    }
                                }
                            }
                            _ => unreachable!(),
                        }
                    }));
                    continue;
                }
                // Preview is cancellable by dropping the UI generation; it never delays lifecycle operations.
                if let Command::Preview(generation, source) = command {
                    let core = core.clone();
                    let tx = tx.clone();
                    if let Some(task) = preview_task.take() {
                        task.abort();
                    }
                    preview_task = Some(tokio::spawn(async move {
                        let result = tokio::time::timeout(
                            std::time::Duration::from_secs(45),
                            core.resolve_magnet_preview(source),
                        )
                        .await;
                        let result = match result {
                            Ok(Ok(p)) => Ok(p),
                            _ => Err(
                                "Metadata could not be resolved. Check the link or try again."
                                    .into(),
                            ),
                        };
                        let _ = tx.send(Message::Preview(generation, result)).await;
                    }));
                    continue;
                }
                let busy = matches!(
                    command,
                    Command::Create(_)
                        | Command::Act(..)
                        | Command::Settings(_)
                        | Command::Limits(..)
                );
                if let Err(error) = execute(&core, command, &tx, &data).await {
                    let _ = tx.send(Message::Error(error.to_string())).await;
                }
                if busy {
                    let _ = tx.send(Message::Finished).await;
                }
            }
            if let Some(task) = browser_bridge {
                task.abort();
                let _ = task.await;
                let _ = tokio::fs::remove_file(fluxion_browser::socket_path()).await;
            }
            let _ = core.pause_all().await;
            forward.abort();
            if let Some(task) = preview_task {
                task.abort();
            }
            if let Some(task) = update_task {
                task.abort();
            }
            if let Some(task) = check_task {
                task.abort();
            }
            let _ = done.send(()).await;
        };
        runtime.spawn(serve);
        Self {
            commands,
            messages,
            stopped,
        }
    }
}

async fn snapshot(
    core: &Arc<FluxionCore>,
    tx: &async_channel::Sender<Message>,
) -> anyhow::Result<()> {
    tx.send(Message::Snapshot(
        core.list_tasks(TaskFilter::default()).await?,
        core.get_settings().await?,
    ))
    .await?;
    Ok(())
}

async fn execute(
    core: &Arc<FluxionCore>,
    command: Command,
    tx: &async_channel::Sender<Message>,
    data: &Path,
) -> anyhow::Result<()> {
    match command {
        Command::Refresh => snapshot(core, tx).await?,
        Command::Detail(id) => {
            tx.send(Message::Detail(
                id,
                core.get_task(id).await?.map(|v| v.redacted()),
            ))
            .await?;
        }
        Command::Bt(id) => {
            tx.send(Message::Bt(id, core.get_task_state(id).await?))
                .await?;
        }
        Command::Create(input) => {
            tx.send(Message::Created(core.create_task(input).await?))
                .await?;
        }
        Command::Act(ids, action) => {
            for id in ids {
                match action {
                    Action::Start => core.start_task(id).await?,
                    Action::Pause => core.pause_task(id).await?,
                    Action::Stop => core.stop_task(id).await?,
                    Action::Trash => {
                        if let Some(detail) = core.get_task(id).await?
                            && crate::model::can_stop(&detail.task.state)
                        {
                            core.stop_task(id).await?;
                        }
                    }
                    Action::Restore => (),
                    Action::Delete(files) => core.delete_task(id, files).await?,
                    Action::Open | Action::Reveal => {
                        let detail = core
                            .get_task(id)
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("Task no longer exists"))?;
                        let path = output_path(&detail)?;
                        let mut process = tokio::process::Command::new("/usr/bin/open");
                        if action == Action::Reveal {
                            process.arg("-R");
                        }
                        if !process.arg("--").arg(path).status().await?.success() {
                            anyhow::bail!("Finder could not open this file");
                        }
                    }
                }
                tx.send(Message::Acted(id, action)).await?;
            }
        }
        Command::Settings(settings) => {
            core.update_settings(settings).await?;
            tx.send(Message::Saved).await?;
        }
        Command::Limits(id, limits) => {
            core.update_task_limits(id, limits).await?;
            tx.send(Message::Detail(
                id,
                core.get_task(id).await?.map(|d| d.redacted()),
            ))
            .await?;
            tx.send(Message::Saved).await?;
        }
        Command::Preferences(prefs) => prefs.save(data).await?,
        Command::Preview(..)
        | Command::CheckUpdate(..)
        | Command::DownloadUpdate(..)
        | Command::InstallUpdate(_) => {
            unreachable!()
        }
    }
    Ok(())
}

pub fn output_path(detail: &TaskDetail) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        detail.task.state == TaskState::Completed,
        "The file is not complete yet"
    );
    let name = detail
        .task
        .file_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No output filename"))?;
    let directory = detail.task.save_dir.canonicalize()?;
    let path = directory.join(sanitize_file_name(name)).canonicalize()?;
    anyhow::ensure!(
        path.starts_with(&directory),
        "The file is outside its download directory"
    );
    Ok(path)
}
