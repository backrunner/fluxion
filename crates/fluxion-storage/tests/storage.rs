use fluxion_core::{
    CreateTaskInput, HeaderPair, HttpMethod, HttpResourceMeta, HttpSegment, HttpSegmentState,
    HttpTaskConfig, ProxyPolicy, TaskCredentials, TaskKind, TaskRateLimit, TaskState, TaskStore,
};
use fluxion_storage::SqliteTaskStore;
use tempfile::tempdir;
use url::Url;

#[tokio::test]
async fn task_credentials_are_removed_with_task() {
    let temp = tempdir().unwrap();
    let store = SqliteTaskStore::connect(temp.path().join("test.sqlite"))
        .await
        .unwrap();
    let detail = store
        .insert_task(CreateTaskInput {
            kind: TaskKind::Http(HttpTaskConfig {
                url: Url::parse("https://example.com/file.bin").unwrap(),
                method: HttpMethod::Get,
                headers: Vec::new(),
                max_connections: Some(16),
                min_split_size: None,
                redirect_limit: 10,
            }),
            save_dir: temp.path().to_path_buf(),
            file_name: Some("file.bin".to_string()),
            limits: TaskRateLimit::default(),
            proxy: ProxyPolicy::UseGlobal,
            credentials: TaskCredentials {
                headers: vec![HeaderPair {
                    name: "Authorization".to_string(),
                    value: "Bearer secret".to_string(),
                }],
                ..Default::default()
            },
        })
        .await
        .unwrap();

    assert_eq!(detail.task.state, TaskState::Queued);
    assert_eq!(
        store
            .get_credentials(detail.task.id)
            .await
            .unwrap()
            .headers
            .len(),
        1
    );

    store.delete_task(detail.task.id).await.unwrap();
    assert!(store.get_task(detail.task.id).await.unwrap().is_none());
}

#[tokio::test]
async fn http_segments_round_trip() {
    let temp = tempdir().unwrap();
    let store = SqliteTaskStore::connect(temp.path().join("test.sqlite"))
        .await
        .unwrap();
    let detail = store
        .insert_task(CreateTaskInput {
            kind: TaskKind::Http(HttpTaskConfig {
                url: Url::parse("https://example.com/file.bin").unwrap(),
                method: HttpMethod::Get,
                headers: Vec::new(),
                max_connections: Some(2),
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

    store
        .update_http_meta(
            detail.task.id,
            HttpResourceMeta {
                final_url: Url::parse("https://example.com/file.bin").unwrap(),
                etag: Some("abc".to_string()),
                last_modified: None,
                content_length: Some(10),
                supports_ranges: true,
                temp_path: Some(temp.path().join("file.bin.fluxionpart")),
            },
        )
        .await
        .unwrap();
    store
        .replace_http_segments(
            detail.task.id,
            vec![HttpSegment {
                index: 0,
                start_byte: 0,
                end_byte: 9,
                downloaded_bytes: 4,
                state: HttpSegmentState::Downloading,
                retry_count: 1,
                last_error: Some("network".to_string()),
            }],
        )
        .await
        .unwrap();

    let segments = store.list_http_segments(detail.task.id).await.unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].downloaded_bytes, 4);
    assert_eq!(segments[0].state, HttpSegmentState::Downloading);
    let meta = store
        .get_http_meta(detail.task.id)
        .await
        .unwrap()
        .expect("http meta");
    assert_eq!(meta.etag.as_deref(), Some("abc"));
    assert_eq!(meta.content_length, Some(10));
}

#[tokio::test]
async fn deleting_task_cascades_protocol_rows() {
    let temp = tempdir().unwrap();
    let store = SqliteTaskStore::connect(temp.path().join("test.sqlite"))
        .await
        .unwrap();
    let detail = store
        .insert_task(CreateTaskInput {
            kind: TaskKind::Http(HttpTaskConfig {
                url: Url::parse("https://example.com/file.bin").unwrap(),
                method: HttpMethod::Get,
                headers: Vec::new(),
                max_connections: Some(2),
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
    store
        .update_http_meta(
            detail.task.id,
            HttpResourceMeta {
                final_url: Url::parse("https://example.com/file.bin").unwrap(),
                etag: None,
                last_modified: None,
                content_length: Some(1),
                supports_ranges: true,
                temp_path: Some(temp.path().join("file.bin.fluxionpart")),
            },
        )
        .await
        .unwrap();
    store
        .replace_http_segments(
            detail.task.id,
            vec![HttpSegment {
                index: 0,
                start_byte: 0,
                end_byte: 0,
                downloaded_bytes: 1,
                state: HttpSegmentState::Completed,
                retry_count: 0,
                last_error: None,
            }],
        )
        .await
        .unwrap();

    store.delete_task(detail.task.id).await.unwrap();

    assert!(store.get_http_meta(detail.task.id).await.unwrap().is_none());
    assert!(
        store
            .list_http_segments(detail.task.id)
            .await
            .unwrap()
            .is_empty()
    );
}
