use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    BtStateSnapshot, CoreEvent, CreateTaskInput, DownloadEngine, DownloadKind, EngineContext,
    EngineExit, EventBus, FluxionError, FluxionErrorKind, Limiters, MagnetPreview, PreparedTask,
    Result, SettingsSnapshot, StateProviderRegistry, TaskControl, TaskDetail, TaskFilter, TaskId,
    TaskRateLimit, TaskState, TaskStore, TaskSummary,
};

pub struct FluxionCore {
    storage: Arc<dyn TaskStore>,
    events: EventBus,
    engines: HashMap<DownloadKind, Arc<dyn DownloadEngine>>,
    active: Arc<Mutex<HashMap<TaskId, Arc<ActiveTask>>>>,
    limiters: Arc<Limiters>,
    state_providers: StateProviderRegistry,
}

struct ActiveTask {
    cancellation: CancellationToken,
    finished: Notify,
    is_finished: AtomicBool,
}

impl ActiveTask {
    fn new(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            finished: Notify::new(),
            is_finished: AtomicBool::new(false),
        }
    }

    fn cancel(&self) {
        self.cancellation.cancel();
    }

    fn mark_finished(&self) {
        self.is_finished.store(true, Ordering::Release);
        self.finished.notify_waiters();
    }

    async fn wait_finished(&self) {
        while !self.is_finished.load(Ordering::Acquire) {
            self.finished.notified().await;
        }
    }
}

impl FluxionCore {
    pub fn new(storage: Arc<dyn TaskStore>, engines: Vec<Arc<dyn DownloadEngine>>) -> Self {
        Self {
            storage,
            events: EventBus::new(1024),
            engines: engines
                .into_iter()
                .map(|engine| (engine.kind(), engine))
                .collect(),
            active: Arc::new(Mutex::new(HashMap::new())),
            limiters: Arc::new(Limiters::default()),
            state_providers: StateProviderRegistry::new(),
        }
    }

    pub async fn initialize(&self) -> Result<()> {
        let settings = self.storage.get_settings().await?;
        self.limiters
            .global_download
            .update_limit(settings.download_limit)
            .await;
        self.limiters
            .global_upload
            .update_limit(settings.upload_limit)
            .await;
        Ok(())
    }

    pub async fn create_task(&self, input: CreateTaskInput) -> Result<TaskId> {
        let detail = self.storage.insert_task(input).await?;
        self.events
            .emit(CoreEvent::TaskCreated(TaskSummary::from(&detail.task)));
        Ok(detail.task.id)
    }

    pub async fn start_task(&self, task_id: TaskId) -> Result<()> {
        if self.active.lock().await.contains_key(&task_id) {
            return Ok(());
        }

        let detail =
            self.storage.get_task(task_id).await?.ok_or_else(|| {
                FluxionError::new(FluxionErrorKind::InvalidConfig, "task not found")
            })?;
        let kind = detail.task.kind.download_kind();
        let engine = self.engines.get(&kind).cloned().ok_or_else(|| {
            FluxionError::new(FluxionErrorKind::Unsupported, "engine not registered")
        })?;

        let cancellation = CancellationToken::new();
        let active_task = Arc::new(ActiveTask::new(cancellation.clone()));
        self.active
            .lock()
            .await
            .insert(task_id, active_task.clone());

        let ctx = EngineContext {
            storage: self.storage.clone(),
            events: self.events.clone(),
            state_providers: self.state_providers.clone(),
        };
        let active = self.active.clone();
        let control = TaskControl::new(
            cancellation,
            detail.task.limits.clone(),
            self.limiters.clone(),
        );

        tokio::spawn(async move {
            let task_id = detail.task.id;
            let result = async {
                ctx.set_state(task_id, TaskState::Resolving).await?;
                let prepared = engine
                    .prepare(
                        ctx.clone(),
                        PreparedTask {
                            task: detail.task,
                            credentials: detail.credentials,
                        },
                    )
                    .await?;
                ctx.set_state(task_id, TaskState::Downloading).await?;
                engine.run(ctx.clone(), prepared, control).await
            }
            .await;

            match result {
                Ok(EngineExit::Completed { file_path }) => {
                    let _ = ctx.set_state(task_id, TaskState::Completed).await;
                    if let Some(file_path) = file_path {
                        ctx.events
                            .emit(CoreEvent::TaskCompleted { task_id, file_path });
                    }
                    // Engine finished; its live state provider is no longer available.
                    ctx.state_providers.remove(task_id).await;
                }
                Ok(EngineExit::Seeding) => {
                    let _ = ctx.set_state(task_id, TaskState::Seeding).await;
                    // Keep the provider alive while seeding.
                }
                Ok(EngineExit::Cancelled) => {
                    ctx.state_providers.remove(task_id).await;
                }
                Err(error) if error.kind() == FluxionErrorKind::Cancelled => {
                    ctx.state_providers.remove(task_id).await;
                }
                Err(error) => {
                    let _ = ctx.fail(task_id, &error).await;
                    ctx.state_providers.remove(task_id).await;
                }
            }
            active_task.mark_finished();
            active.lock().await.remove(&task_id);
        });

        Ok(())
    }

    pub async fn pause_task(&self, task_id: TaskId) -> Result<()> {
        let active = { self.active.lock().await.get(&task_id).cloned() };
        if let Some(active) = active {
            active.cancel();
            active.wait_finished().await;
        }
        self.set_user_requested_state(task_id, TaskState::Paused)
            .await?;
        Ok(())
    }

    pub async fn stop_task(&self, task_id: TaskId) -> Result<()> {
        let active = { self.active.lock().await.get(&task_id).cloned() };
        if let Some(active) = active {
            active.cancel();
            active.wait_finished().await;
        }
        self.set_user_requested_state(task_id, TaskState::Stopped)
            .await?;
        Ok(())
    }

    pub async fn delete_task(&self, task_id: TaskId, delete_files: bool) -> Result<()> {
        let active = { self.active.lock().await.get(&task_id).cloned() };
        if let Some(active) = active {
            active.cancel();
            active.wait_finished().await;
        }
        let paths = if delete_files {
            self.storage.output_paths(task_id).await?
        } else {
            Vec::new()
        };
        self.storage.delete_task(task_id).await?;
        for path in paths {
            remove_file_if_exists(path).await?;
        }
        Ok(())
    }

    async fn set_user_requested_state(&self, task_id: TaskId, state: TaskState) -> Result<()> {
        let detail =
            self.storage.get_task(task_id).await?.ok_or_else(|| {
                FluxionError::new(FluxionErrorKind::InvalidConfig, "task not found")
            })?;
        if matches!(detail.task.state, TaskState::Completed | TaskState::Failed) {
            return Ok(());
        }
        self.storage.update_state(task_id, state.clone()).await?;
        self.events
            .emit(CoreEvent::TaskStateChanged { task_id, state });
        Ok(())
    }

    pub async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<TaskSummary>> {
        self.storage.list_tasks(filter).await
    }

    pub async fn get_task(&self, task_id: TaskId) -> Result<Option<TaskDetail>> {
        self.storage.get_task(task_id).await
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    pub async fn get_settings(&self) -> Result<SettingsSnapshot> {
        self.storage.get_settings().await
    }

    pub async fn update_settings(&self, settings: SettingsSnapshot) -> Result<()> {
        self.limiters
            .global_download
            .update_limit(settings.download_limit)
            .await;
        self.limiters
            .global_upload
            .update_limit(settings.upload_limit)
            .await;
        self.storage.update_settings(settings.clone()).await?;
        self.events.emit(CoreEvent::SettingsChanged(settings));
        Ok(())
    }

    /// Pause every currently active task. Tasks that are not running are
    /// left untouched (their state already reflects user intent).
    pub async fn pause_all(&self) -> Result<()> {
        let active_ids: Vec<TaskId> = self.active.lock().await.keys().copied().collect();
        for task_id in active_ids {
            // pause_task is idempotent and safe to call per id.
            if let Err(error) = self.pause_task(task_id).await {
                tracing::warn!(?task_id, ?error, "pause_all: failed to pause task");
            }
        }
        Ok(())
    }

    /// Start every task that is currently idle (Paused / Stopped / Queued /
    /// Failed). Already-active tasks are skipped by `start_task`.
    pub async fn start_all(&self) -> Result<()> {
        let idle = self
            .list_tasks(TaskFilter::default())
            .await?
            .into_iter()
            .filter(|summary| {
                matches!(
                    summary.state,
                    TaskState::Paused | TaskState::Stopped | TaskState::Queued | TaskState::Failed
                )
            })
            .map(|summary| summary.id)
            .collect::<Vec<_>>();
        for task_id in idle {
            if let Err(error) = self.start_task(task_id).await {
                tracing::warn!(?task_id, ?error, "start_all: failed to start task");
            }
        }
        Ok(())
    }

    /// Delete every finished task (Completed / Failed / Stopped). When
    /// `delete_files` is true the downloaded files are removed too. A single
    /// task failing to delete does not abort the rest; the first encountered
    /// error is returned to the caller.
    pub async fn clear_finished(&self, delete_files: bool) -> Result<()> {
        let finished = self
            .list_tasks(TaskFilter::default())
            .await?
            .into_iter()
            .filter(|summary| {
                matches!(
                    summary.state,
                    TaskState::Completed | TaskState::Failed | TaskState::Stopped
                )
            })
            .map(|summary| summary.id)
            .collect::<Vec<_>>();
        let mut first_error: Option<FluxionError> = None;
        for task_id in finished {
            if let Err(error) = self.delete_task(task_id, delete_files).await {
                tracing::warn!(?task_id, ?error, "clear_finished: failed to delete task");
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    /// Update a task's rate limits. The new limits are persisted immediately.
    /// If the task is currently running, the in-flight `TaskControl` keeps its
    /// old limiters (they are private to the spawned engine future); the new
    /// limits take effect the next time the task is started. Callers should
    /// surface this caveat to the user.
    pub async fn update_task_limits(&self, task_id: TaskId, limits: TaskRateLimit) -> Result<()> {
        let mut detail =
            self.storage.get_task(task_id).await?.ok_or_else(|| {
                FluxionError::new(FluxionErrorKind::InvalidConfig, "task not found")
            })?;
        detail.task.limits = limits;
        self.storage.update_task(detail.task).await?;
        Ok(())
    }

    /// Read the live, protocol-level state snapshot for an active task (e.g.
    /// BitTorrent file/piece state). Returns `Ok(None)` when the task is not
    /// running or its engine does not expose a state provider.
    pub async fn get_task_state(&self, task_id: TaskId) -> Result<Option<BtStateSnapshot>> {
        match self.state_providers.get(task_id).await {
            Some(provider) => {
                let snapshot = provider.snapshot().await?;
                Ok(Some(snapshot))
            }
            None => Ok(None),
        }
    }

    /// Resolve a magnet link into torrent metadata (name + file list) before a
    /// task is created, so the UI can preview and let the user pick files.
    /// Routes to the BitTorrent engine; other engines do not support this.
    /// This is a read-only probe: it does not persist a task or start a
    /// download. Network-bound; may time out on dead swarms.
    pub async fn resolve_magnet_preview(&self, magnet: String) -> Result<MagnetPreview> {
        let engine = self.engines.get(&DownloadKind::Bt).ok_or_else(|| {
            FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "BitTorrent engine is not available",
            )
        })?;
        engine.resolve_magnet_preview(&magnet).await
    }
}

impl Default for FluxionCore {
    fn default() -> Self {
        panic!("use FluxionCore::new with storage and engines")
    }
}

pub fn new_task_id() -> TaskId {
    Uuid::new_v4()
}

async fn remove_file_if_exists(path: PathBuf) -> Result<()> {
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
