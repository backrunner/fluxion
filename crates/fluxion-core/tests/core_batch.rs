use std::sync::Arc;

use async_trait::async_trait;
use fluxion_core::{
    CreateTaskInput, DownloadEngine, DownloadKind, EngineContext, EngineExit, HttpMethod,
    HttpTaskConfig, PreparedTask, ProxyPolicy, Result, TaskControl, TaskCredentials, TaskKind,
    TaskRateLimit, TaskState, TaskStore,
};
use fluxion_storage::SqliteTaskStore;
use tempfile::tempdir;
use url::Url;

#[tokio::test]
async fn clear_finished_removes_completed_failed_stopped_tasks() {
    let temp = tempdir().unwrap();
    let store = Arc::new(
        SqliteTaskStore::connect(temp.path().join("test.sqlite"))
            .await
            .unwrap(),
    );
    let core = fluxion_core::FluxionCore::new(store.clone(), vec![Arc::new(NoopEngine)]);

    let completed = create_task(&core, temp.path().to_path_buf()).await;
    let stopped = create_task(&core, temp.path().to_path_buf()).await;
    let paused = create_task(&core, temp.path().to_path_buf()).await;

    // Place tasks into target states directly through storage so the test does
    // not race the spawned engine future.
    store
        .update_state(completed, TaskState::Completed)
        .await
        .unwrap();
    store
        .update_state(stopped, TaskState::Stopped)
        .await
        .unwrap();
    store.update_state(paused, TaskState::Paused).await.unwrap();

    core.clear_finished(false).await.unwrap();

    let remaining: Vec<TaskState> = core
        .list_tasks(fluxion_core::TaskFilter::default())
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.state)
        .collect();
    // Completed + Stopped removed; Paused stays.
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0], TaskState::Paused);
}

#[tokio::test]
async fn start_all_resumes_idle_tasks() {
    let temp = tempdir().unwrap();
    let store = Arc::new(
        SqliteTaskStore::connect(temp.path().join("test.sqlite"))
            .await
            .unwrap(),
    );
    let core = Arc::new(fluxion_core::FluxionCore::new(
        store,
        vec![Arc::new(NoopEngine)],
    ));

    let first = create_task(&core, temp.path().to_path_buf()).await;
    let second = create_task(&core, temp.path().to_path_buf()).await;

    core.start_all().await.unwrap();

    // The NoopEngine completes inside a spawned task; yield the runtime until
    // both reach a terminal state.
    wait_for_states(&core, &[first, second], TaskState::Completed).await;
}

#[tokio::test]
async fn update_task_limits_persists_new_limits() {
    let temp = tempdir().unwrap();
    let store = Arc::new(
        SqliteTaskStore::connect(temp.path().join("test.sqlite"))
            .await
            .unwrap(),
    );
    let core = fluxion_core::FluxionCore::new(store, vec![Arc::new(NoopEngine)]);

    let id = create_task(&core, temp.path().to_path_buf()).await;
    core.update_task_limits(
        id,
        TaskRateLimit {
            download_bytes_per_second: Some(123),
            upload_bytes_per_second: Some(456),
        },
    )
    .await
    .unwrap();

    let detail = core.get_task(id).await.unwrap().unwrap();
    assert_eq!(detail.task.limits.download_bytes_per_second, Some(123));
    assert_eq!(detail.task.limits.upload_bytes_per_second, Some(456));
}

/// Poll the task list until every given task reaches the expected state, or
/// time out. The NoopEngine completes asynchronously inside a spawned task,
/// so callers must wait rather than assert synchronously.
async fn wait_for_states(
    core: &fluxion_core::FluxionCore,
    ids: &[uuid::Uuid],
    expected: TaskState,
) {
    for _ in 0..200 {
        let summaries = core
            .list_tasks(fluxion_core::TaskFilter::default())
            .await
            .unwrap();
        let all_ready = ids.iter().all(|id| {
            summaries
                .iter()
                .find(|s| s.id == *id)
                .map(|s| s.state == expected)
                .unwrap_or(false)
        });
        if all_ready {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("tasks did not reach {expected:?} in time");
}

async fn create_task(core: &fluxion_core::FluxionCore, save_dir: std::path::PathBuf) -> uuid::Uuid {
    core.create_task(CreateTaskInput {
        kind: TaskKind::Http(HttpTaskConfig {
            url: Url::parse("https://example.com/file.bin").unwrap(),
            method: HttpMethod::Get,
            headers: Vec::new(),
            max_connections: Some(1),
            min_split_size: None,
            redirect_limit: 10,
        }),
        save_dir,
        file_name: Some("file.bin".to_string()),
        limits: TaskRateLimit::default(),
        proxy: ProxyPolicy::UseGlobal,
        credentials: TaskCredentials::default(),
    })
    .await
    .unwrap()
}

struct NoopEngine;

#[async_trait]
impl DownloadEngine for NoopEngine {
    fn kind(&self) -> DownloadKind {
        DownloadKind::Http
    }

    async fn prepare(&self, _ctx: EngineContext, task: PreparedTask) -> Result<PreparedTask> {
        Ok(task)
    }

    async fn run(
        &self,
        _ctx: EngineContext,
        _task: PreparedTask,
        _control: TaskControl,
    ) -> Result<EngineExit> {
        Ok(EngineExit::Completed { file_path: None })
    }
}
