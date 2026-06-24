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
        let task_download = SharedRateLimiter::default();
        let task_upload = SharedRateLimiter::default();
        let this = Self {
            cancellation,
            task_download,
            task_upload,
            global,
        };
        let clone = this.clone();
        tokio::spawn(async move {
            clone
                .task_download
                .update_limit(limits.download_bytes_per_second)
                .await;
            clone
                .task_upload
                .update_limit(limits.upload_bytes_per_second)
                .await;
        });
        this
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub async fn acquire_download(&self, bytes: u64) {
        self.task_download.acquire(bytes).await;
        self.global.global_download.acquire(bytes).await;
    }

    pub async fn acquire_upload(&self, bytes: u64) {
        self.task_upload.acquire(bytes).await;
        self.global.global_upload.acquire(bytes).await;
    }
}
