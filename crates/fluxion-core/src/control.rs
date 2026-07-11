use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::{Limiters, SharedRateLimiter, TaskRateLimit};

#[derive(Clone)]
pub struct TaskControl {
    cancellation: CancellationToken,
    task_download: SharedRateLimiter,
    task_upload: SharedRateLimiter,
    global: Arc<Limiters>,
}

impl TaskControl {
    pub fn new(
        cancellation: CancellationToken,
        limits: TaskRateLimit,
        global: Arc<Limiters>,
    ) -> Self {
        Self {
            cancellation,
            // Synchronous construction: the limits are in force before the
            // engine future is ever spawned (no unlimited startup window).
            task_download: SharedRateLimiter::with_limit(limits.download_bytes_per_second),
            task_upload: SharedRateLimiter::with_limit(limits.upload_bytes_per_second),
            global,
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Apply new task-level limits to a running task.
    pub async fn update_limits(&self, limits: TaskRateLimit) {
        self.task_download
            .update_limit(limits.download_bytes_per_second)
            .await;
        self.task_upload
            .update_limit(limits.upload_bytes_per_second)
            .await;
    }

    /// Task-level limiter first, then the global one, so a task can neither
    /// exceed its own cap nor break the global budget. Returns early when the
    /// task is cancelled so pause/stop never hang on a starved limiter.
    pub async fn acquire_download(&self, bytes: u64) {
        tokio::select! {
            _ = async {
                self.task_download.acquire(bytes).await;
                self.global.global_download.acquire(bytes).await;
            } => {}
            _ = self.cancellation.cancelled() => {}
        }
    }

    pub async fn acquire_upload(&self, bytes: u64) {
        tokio::select! {
            _ = async {
                self.task_upload.acquire(bytes).await;
                self.global.global_upload.acquire(bytes).await;
            } => {}
            _ = self.cancellation.cancelled() => {}
        }
    }
}
