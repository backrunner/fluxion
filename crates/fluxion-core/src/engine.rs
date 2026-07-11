use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{
    BtStateSnapshot, CoreEvent, DownloadKind, DownloadTask, EventBus, FluxionError,
    FluxionErrorKind, MagnetPreview, Result, TaskControl, TaskCredentials, TaskId, TaskProgress,
    TaskState, TaskStore,
};

/// Optional, engine-specific runtime state provider. Implemented by engines
/// that can surface protocol-level detail (e.g. BitTorrent file/piece state)
/// for an active task. HTTP/FTP/SFTP engines do not implement this.
#[async_trait]
pub trait EngineStateProvider: Send + Sync {
    async fn snapshot(&self) -> Result<BtStateSnapshot>;
}

/// Shared registry of active task state providers, keyed by task id. Populated
/// by engines during `run()` via `EngineContext::register_state_provider`, and
/// read by `FluxionCore::get_task_state` to answer UI detail queries.
#[derive(Clone, Default)]
pub struct StateProviderRegistry {
    inner: Arc<Mutex<HashMap<TaskId, Arc<dyn EngineStateProvider>>>>,
}

impl StateProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, task_id: TaskId, provider: Arc<dyn EngineStateProvider>) {
        self.inner.lock().await.insert(task_id, provider);
    }

    pub async fn get(&self, task_id: TaskId) -> Option<Arc<dyn EngineStateProvider>> {
        self.inner.lock().await.get(&task_id).cloned()
    }

    pub async fn remove(&self, task_id: TaskId) {
        self.inner.lock().await.remove(&task_id);
    }
}

#[derive(Clone)]
pub struct EngineContext {
    pub storage: Arc<dyn TaskStore>,
    pub events: EventBus,
    pub(crate) state_providers: StateProviderRegistry,
}

impl EngineContext {
    /// Register a runtime state provider for an active task. Engines call this
    /// at the start of `run()` if they can surface protocol-level detail.
    pub async fn register_state_provider(
        &self,
        task_id: TaskId,
        provider: Arc<dyn EngineStateProvider>,
    ) {
        self.state_providers.register(task_id, provider).await;
    }

    /// Read the state provider for a task, if any (used by Core to answer
    /// `get_task_state` queries).
    pub async fn state_provider(&self, task_id: TaskId) -> Option<Arc<dyn EngineStateProvider>> {
        self.state_providers.get(task_id).await
    }

    pub async fn set_state(&self, task_id: TaskId, state: TaskState) -> Result<()> {
        self.storage.update_state(task_id, state.clone()).await?;
        self.events
            .emit(CoreEvent::TaskStateChanged { task_id, state });
        Ok(())
    }

    pub async fn progress(
        &self,
        task_id: TaskId,
        downloaded_bytes: u64,
        uploaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<()> {
        self.storage
            .update_progress(task_id, downloaded_bytes, uploaded_bytes, total_bytes)
            .await?;
        self.events.emit(CoreEvent::TaskProgress(TaskProgress {
            task_id,
            downloaded_bytes,
            uploaded_bytes,
            total_bytes,
        }));
        Ok(())
    }

    pub async fn fail(&self, task_id: TaskId, error: &FluxionError) -> Result<()> {
        let message = error.to_string();
        self.storage
            .update_error(task_id, Some(message.clone()))
            .await?;
        // Emit the error details alongside the state change so UIs subscribed
        // to the event stream get the message without an extra fetch.
        self.events
            .emit(CoreEvent::TaskError(crate::TaskErrorEvent {
                task_id,
                message,
            }));
        self.set_state(task_id, TaskState::Failed).await
    }
}

#[derive(Debug, Clone)]
pub struct PreparedTask {
    pub task: DownloadTask,
    pub credentials: TaskCredentials,
}

#[derive(Debug, Clone)]
pub enum EngineExit {
    Completed { file_path: Option<PathBuf> },
    Seeding,
    Cancelled,
}

#[async_trait]
pub trait DownloadEngine: Send + Sync {
    fn kind(&self) -> DownloadKind;

    async fn prepare(&self, ctx: EngineContext, task: PreparedTask) -> Result<PreparedTask>;

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit>;

    /// Resolve a magnet link into torrent metadata (name + file list) without
    /// starting a download, so the UI can preview and let the user pick files
    /// before task creation. Only the BT engine implements this; the default
    /// returns an error for every other engine so Core can stay trait-routed.
    async fn resolve_magnet_preview(&self, _magnet: &str) -> Result<MagnetPreview> {
        Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "magnet preview is not supported by this engine",
        ))
    }
}
