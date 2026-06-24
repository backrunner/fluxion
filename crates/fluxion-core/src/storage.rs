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
