use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use fluxion_core::{
    CreateTaskInput, DownloadEngine, DownloadKind, EngineContext, EngineExit, HttpMethod,
    HttpTaskConfig, PreparedTask, ProxyPolicy, Result, TaskControl, TaskCredentials, TaskKind,
    TaskRateLimit, TaskStore,
};
use fluxion_storage::SqliteTaskStore;
use tempfile::tempdir;
use tokio::sync::Notify;
use url::Url;

#[tokio::test]
async fn delete_task_removes_output_files_when_requested() {
    let temp = tempdir().unwrap();
    let store = Arc::new(
        SqliteTaskStore::connect(temp.path().join("test.sqlite"))
            .await
            .unwrap(),
    );
    let core = fluxion_core::FluxionCore::new(store, vec![Arc::new(NoopEngine)]);
    let output = temp.path().join("file.bin");
    let part = temp.path().join("file.bin.fluxionpart");
    tokio::fs::write(&output, b"done").await.unwrap();
    tokio::fs::write(&part, b"part").await.unwrap();
    let id = core
        .create_task(CreateTaskInput {
            kind: TaskKind::Http(HttpTaskConfig {
                url: Url::parse("https://example.com/file.bin").unwrap(),
                method: HttpMethod::Get,
                headers: Vec::new(),
                max_connections: Some(1),
                min_split_size: None,
                redirect_limit: 10,
            }),
            save_dir: temp.path().to_path_buf(),
            file_name: Some("file.bin".to_string()),
            limits: TaskRateLimit::default(),
            proxy: ProxyPolicy::UseGlobal,
            credentials: TaskCredentials::default(),
        })
        .await
        .unwrap();

    core.delete_task(id, true).await.unwrap();

    assert!(!output.exists());
    assert!(!part.exists());
}

#[tokio::test]
async fn pause_waits_for_engine_cancellation_flush() {
    let temp = tempdir().unwrap();
    let store = Arc::new(
        SqliteTaskStore::connect(temp.path().join("test.sqlite"))
            .await
            .unwrap(),
    );
    let engine = Arc::new(BlockingEngine::default());
    let core = fluxion_core::FluxionCore::new(store.clone(), vec![engine.clone()]);
    let id = create_http_task(&core, temp.path().to_path_buf()).await;
    core.start_task(id).await.unwrap();
    engine.started.notified().await;
    let pause = tokio::spawn({
        let core = core;
        async move {
            core.pause_task(id).await.unwrap();
        }
    });
    engine.cancel_seen.notified().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(!pause.is_finished());
    engine.allow_finish.notify_waiters();
    pause.await.unwrap();
    assert!(engine.flushed.load(Ordering::SeqCst));
    assert_eq!(
        store.get_task(id).await.unwrap().unwrap().task.state,
        fluxion_core::TaskState::Paused
    );
}

#[tokio::test]
async fn delete_waits_for_engine_before_removing_files() {
    let temp = tempdir().unwrap();
    let store = Arc::new(
        SqliteTaskStore::connect(temp.path().join("test.sqlite"))
            .await
            .unwrap(),
    );
    let engine = Arc::new(BlockingEngine::default());
    let core = Arc::new(fluxion_core::FluxionCore::new(store, vec![engine.clone()]));
    let output = temp.path().join("file.bin");
    tokio::fs::write(&output, b"done").await.unwrap();
    let id = create_http_task(&core, temp.path().to_path_buf()).await;
    core.start_task(id).await.unwrap();
    engine.started.notified().await;
    let delete = tokio::spawn({
        let core = core.clone();
        async move {
            core.delete_task(id, true).await.unwrap();
        }
    });
    engine.cancel_seen.notified().await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert!(output.exists());
    assert!(!delete.is_finished());
    engine.allow_finish.notify_waiters();
    delete.await.unwrap();
    assert!(!output.exists());
}

async fn create_http_task(
    core: &fluxion_core::FluxionCore,
    save_dir: std::path::PathBuf,
) -> uuid::Uuid {
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

#[derive(Default)]
struct BlockingEngine {
    started: Notify,
    cancel_seen: Notify,
    allow_finish: Notify,
    flushed: AtomicBool,
}

#[async_trait]
impl DownloadEngine for BlockingEngine {
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
        control: TaskControl,
    ) -> Result<EngineExit> {
        self.started.notify_waiters();
        control.cancellation_token().cancelled().await;
        self.cancel_seen.notify_waiters();
        self.allow_finish.notified().await;
        self.flushed.store(true, Ordering::SeqCst);
        Err(fluxion_core::FluxionError::cancelled())
    }
}
