use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{SettingsSnapshot, TaskId, TaskState, TaskSummary};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreEvent {
    TaskCreated(TaskSummary),
    TaskStateChanged { task_id: TaskId, state: TaskState },
    TaskProgress(TaskProgress),
    TaskSpeed(TaskSpeed),
    TaskError(TaskErrorEvent),
    TaskCompleted { task_id: TaskId, file_path: PathBuf },
    SettingsChanged(SettingsSnapshot),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: TaskId,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpeed {
    pub task_id: TaskId,
    pub download_bytes_per_second: u64,
    pub upload_bytes_per_second: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskErrorEvent {
    pub task_id: TaskId,
    pub message: String,
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<CoreEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn emit(&self, event: CoreEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.sender.subscribe()
    }
}
