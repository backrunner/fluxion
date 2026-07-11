use async_trait::async_trait;

use crate::{
    CreateTaskInput, DownloadTask, HttpResourceMeta, HttpSegment, Result, SettingsSnapshot,
    TaskCredentials, TaskDetail, TaskFilter, TaskId, TaskState, TaskSummary,
};

#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn insert_task(&self, input: CreateTaskInput) -> Result<TaskDetail>;
    async fn get_task(&self, task_id: TaskId) -> Result<Option<TaskDetail>>;
    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<TaskSummary>>;
    async fn update_state(&self, task_id: TaskId, state: TaskState) -> Result<()>;
    async fn update_progress(
        &self,
        task_id: TaskId,
        downloaded_bytes: u64,
        uploaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<()>;
    async fn update_task(&self, task: DownloadTask) -> Result<()>;
    async fn update_error(&self, task_id: TaskId, message: Option<String>) -> Result<()>;
    async fn delete_task(&self, task_id: TaskId) -> Result<()>;
    async fn output_paths(&self, task_id: TaskId) -> Result<Vec<std::path::PathBuf>>;
    async fn get_credentials(&self, task_id: TaskId) -> Result<TaskCredentials>;
    async fn get_settings(&self) -> Result<SettingsSnapshot>;
    async fn update_settings(&self, settings: SettingsSnapshot) -> Result<()>;
    async fn get_http_meta(&self, task_id: TaskId) -> Result<Option<HttpResourceMeta>>;
    async fn update_http_meta(&self, task_id: TaskId, meta: HttpResourceMeta) -> Result<()>;
    async fn replace_http_segments(
        &self,
        task_id: TaskId,
        segments: Vec<HttpSegment>,
    ) -> Result<()>;
    async fn list_http_segments(&self, task_id: TaskId) -> Result<Vec<HttpSegment>>;
    async fn update_http_segment(&self, task_id: TaskId, segment: HttpSegment) -> Result<()>;
}

/// Storage for sensitive credential payloads (design §7.3). Implementations
/// keep the secret material out of the regular database — the store is keyed
/// by an opaque reference (e.g. the task id) and the database persists only
/// that reference. macOS uses the Keychain (`fluxion-platform`); tests use an
/// in-memory implementation.
#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn put(&self, secret_ref: &str, value: &str) -> Result<()>;
    async fn get(&self, secret_ref: &str) -> Result<Option<String>>;
    async fn delete(&self, secret_ref: &str) -> Result<()>;
}

/// In-memory secret store for tests and non-macOS fallbacks. Secrets live
/// only for the process lifetime.
#[derive(Default)]
pub struct MemorySecretStore {
    inner: tokio::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[async_trait]
impl SecretStore for MemorySecretStore {
    async fn put(&self, secret_ref: &str, value: &str) -> Result<()> {
        self.inner
            .lock()
            .await
            .insert(secret_ref.to_string(), value.to_string());
        Ok(())
    }

    async fn get(&self, secret_ref: &str) -> Result<Option<String>> {
        Ok(self.inner.lock().await.get(secret_ref).cloned())
    }

    async fn delete(&self, secret_ref: &str) -> Result<()> {
        self.inner.lock().await.remove(secret_ref);
        Ok(())
    }
}
