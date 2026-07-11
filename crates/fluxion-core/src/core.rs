use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::{Mutex, watch};
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
    speed_sampler_started: std::sync::atomic::AtomicBool,
}

struct ActiveTask {
    cancellation: CancellationToken,
    control: TaskControl,
    // `watch` (not `Notify`) so a finish signal sent before a waiter
    // subscribes is never lost — the value persists.
    finished: watch::Sender<bool>,
}

impl ActiveTask {
    fn new(cancellation: CancellationToken, control: TaskControl) -> Self {
        Self {
            cancellation,
            control,
            finished: watch::Sender::new(false),
        }
    }

    fn cancel(&self) {
        self.cancellation.cancel();
    }

    fn mark_finished(&self) {
        let _ = self.finished.send(true);
    }

    async fn wait_finished(&self) {
        let mut rx = self.finished.subscribe();
        // Completes immediately if already finished; otherwise waits for the
        // send. No check-then-wait gap.
        let _ = rx.wait_for(|finished| *finished).await;
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
            speed_sampler_started: std::sync::atomic::AtomicBool::new(false),
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
        self.recover_interrupted_tasks().await?;
        self.spawn_speed_sampler();
        Ok(())
    }

    /// Emit `TaskSpeed` events by sampling progress deltas once per second
    /// (design §3.4: speed aggregated at 1 s). Runs for the lifetime of the
    /// event bus; guarded so repeated `initialize` calls spawn only one.
    fn spawn_speed_sampler(&self) {
        if self
            .speed_sampler_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let events = self.events.clone();
        let mut rx = self.events.subscribe();
        tokio::spawn(async move {
            struct Track {
                latest_down: u64,
                latest_up: u64,
                sampled_down: u64,
                sampled_up: u64,
                last_emitted_nonzero: bool,
            }
            let mut tracks: HashMap<TaskId, Track> = HashMap::new();
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Ok(CoreEvent::TaskProgress(progress)) => {
                                let entry = tracks.entry(progress.task_id).or_insert(Track {
                                    latest_down: progress.downloaded_bytes,
                                    latest_up: progress.uploaded_bytes,
                                    sampled_down: progress.downloaded_bytes,
                                    sampled_up: progress.uploaded_bytes,
                                    last_emitted_nonzero: false,
                                });
                                entry.latest_down = progress.downloaded_bytes;
                                entry.latest_up = progress.uploaded_bytes;
                            }
                            Ok(CoreEvent::TaskStateChanged { task_id, state }) => {
                                let still_active = matches!(
                                    state,
                                    TaskState::Downloading
                                        | TaskState::Resolving
                                        | TaskState::Verifying
                                        | TaskState::Seeding
                                );
                                if !still_active
                                    && let Some(track) = tracks.remove(&task_id)
                                    && track.last_emitted_nonzero
                                {
                                    events.emit(CoreEvent::TaskSpeed(crate::TaskSpeed {
                                        task_id,
                                        download_bytes_per_second: 0,
                                        upload_bytes_per_second: 0,
                                    }));
                                }
                            }
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = tick.tick() => {
                        for (task_id, track) in tracks.iter_mut() {
                            let down = track.latest_down.saturating_sub(track.sampled_down);
                            let up = track.latest_up.saturating_sub(track.sampled_up);
                            track.sampled_down = track.latest_down;
                            track.sampled_up = track.latest_up;
                            // Skip repeating zero-speed events for idle tasks,
                            // but always send the transition to zero once.
                            if down > 0 || up > 0 || track.last_emitted_nonzero {
                                events.emit(CoreEvent::TaskSpeed(crate::TaskSpeed {
                                    task_id: *task_id,
                                    download_bytes_per_second: down,
                                    upload_bytes_per_second: up,
                                }));
                                track.last_emitted_nonzero = down > 0 || up > 0;
                            }
                        }
                    }
                }
            }
        });
    }

    /// Crash recovery: tasks left in an in-flight state by a previous process
    /// (crash or hard kill) have no running engine anymore. Reset them to
    /// `Paused` so the UI reflects reality and the user can resume; segment
    /// data in storage is untouched, so resuming continues from the last
    /// persisted offsets.
    async fn recover_interrupted_tasks(&self) -> Result<()> {
        let tasks = self.storage.list_tasks(TaskFilter::default()).await?;
        for summary in tasks {
            if matches!(
                summary.state,
                TaskState::Downloading
                    | TaskState::Resolving
                    | TaskState::Verifying
                    | TaskState::Seeding
            ) {
                tracing::info!(task_id = ?summary.id, state = ?summary.state, "recovering interrupted task as paused");
                self.storage
                    .update_state(summary.id, TaskState::Paused)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn create_task(&self, mut input: CreateTaskInput) -> Result<TaskId> {
        input.isolate_sensitive_headers();
        let detail = self.storage.insert_task(input).await?;
        self.events
            .emit(CoreEvent::TaskCreated(TaskSummary::from(&detail.task)));
        Ok(detail.task.id)
    }

    pub async fn start_task(&self, task_id: TaskId) -> Result<()> {
        // Fetch the task first so we can build the control with its limits,
        // then claim the active slot atomically (check + insert under one
        // lock hold) so two concurrent starts can never both spawn engines.
        let detail =
            self.storage.get_task(task_id).await?.ok_or_else(|| {
                FluxionError::new(FluxionErrorKind::InvalidConfig, "task not found")
            })?;
        if detail.task.state == TaskState::Completed {
            // A completed task's temp file is gone; re-running the engine
            // would clobber the finished file with a fresh sparse one.
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "task is already completed",
            ));
        }
        let kind = detail.task.kind.download_kind();
        let engine = self.engines.get(&kind).cloned().ok_or_else(|| {
            FluxionError::new(FluxionErrorKind::Unsupported, "engine not registered")
        })?;

        let cancellation = CancellationToken::new();
        let control = TaskControl::new(
            cancellation.clone(),
            detail.task.limits.clone(),
            self.limiters.clone(),
        );
        let active_task = Arc::new(ActiveTask::new(cancellation, control.clone()));
        {
            let mut active = self.active.lock().await;
            if active.contains_key(&task_id) {
                return Ok(());
            }
            active.insert(task_id, active_task.clone());
        }

        let ctx = EngineContext {
            storage: self.storage.clone(),
            events: self.events.clone(),
            state_providers: self.state_providers.clone(),
        };
        let active = self.active.clone();

        tokio::spawn(async move {
            let task_id = detail.task.id;
            // Run the engine on an inner task so a panic surfaces as a
            // JoinError instead of leaving a zombie entry in `active` that
            // blocks pause/stop/delete forever.
            let inner_ctx = ctx.clone();
            let inner = tokio::spawn(async move {
                inner_ctx.set_state(task_id, TaskState::Resolving).await?;
                let prepared = engine
                    .prepare(
                        inner_ctx.clone(),
                        PreparedTask {
                            task: detail.task,
                            credentials: detail.credentials,
                        },
                    )
                    .await?;
                inner_ctx.set_state(task_id, TaskState::Downloading).await?;
                engine.run(inner_ctx, prepared, control).await
            });
            let result = match inner.await {
                Ok(result) => result,
                Err(join_error) => Err(FluxionError::new(
                    FluxionErrorKind::Unknown,
                    format!("engine task aborted: {join_error}"),
                )),
            };

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
            // Remove from `active` BEFORE signalling finished, and only if the
            // entry is still this generation — a fast pause→start may already
            // have installed a newer ActiveTask under the same id.
            {
                let mut map = active.lock().await;
                if map
                    .get(&task_id)
                    .is_some_and(|entry| Arc::ptr_eq(entry, &active_task))
                {
                    map.remove(&task_id);
                }
            }
            active_task.mark_finished();
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
        // Persist first so a storage failure cannot leave the running process
        // using settings that will silently disappear after restart.
        self.storage.update_settings(settings.clone()).await?;
        self.limiters
            .global_download
            .update_limit(settings.download_limit)
            .await;
        self.limiters
            .global_upload
            .update_limit(settings.upload_limit)
            .await;
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

    /// Update a task's rate limits. The new limits are persisted immediately
    /// and, when the task is running, pushed into its live `TaskControl` so
    /// they take effect without a restart.
    pub async fn update_task_limits(&self, task_id: TaskId, limits: TaskRateLimit) -> Result<()> {
        let mut detail =
            self.storage.get_task(task_id).await?.ok_or_else(|| {
                FluxionError::new(FluxionErrorKind::InvalidConfig, "task not found")
            })?;
        detail.task.limits = limits.clone();
        self.storage.update_task(detail.task).await?;
        let active = { self.active.lock().await.get(&task_id).cloned() };
        if let Some(active) = active {
            active.control.update_limits(limits).await;
        }
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
