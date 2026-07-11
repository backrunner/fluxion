use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use fluxion_core::{
    CreateTaskInput, HeaderPair, HttpMethod, HttpResourceMeta, HttpSegment, HttpSegmentState,
    HttpTaskConfig, ProxyPolicy, Result, SECRET_HTTP_SOURCE_URL, SecretStore, SettingsSnapshot,
    TaskCredentials, TaskKind, TaskRateLimit, TaskState, TaskStore,
};
use fluxion_storage::SqliteTaskStore;
use tempfile::tempdir;
use url::Url;

#[derive(Default)]
struct MemorySecretStore(Mutex<HashMap<String, String>>);

#[async_trait]
impl SecretStore for MemorySecretStore {
    async fn put(&self, secret_ref: &str, value: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .insert(secret_ref.to_string(), value.to_string());
        Ok(())
    }

    async fn get(&self, secret_ref: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(secret_ref).cloned())
    }

    async fn delete(&self, secret_ref: &str) -> Result<()> {
        self.0.lock().unwrap().remove(secret_ref);
        Ok(())
    }
}

#[tokio::test]
async fn reconnect_with_secret_store_migrates_plaintext_sources_and_settings() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("test.sqlite");
    let plain = SqliteTaskStore::connect(&path).await.unwrap();
    let signed_url = "https://example.com/file.bin?X-Amz-Signature=top-secret";
    let detail = plain
        .insert_task(CreateTaskInput {
            kind: TaskKind::Http(HttpTaskConfig {
                url: Url::parse(signed_url).unwrap(),
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
            credentials: TaskCredentials::default(),
        })
        .await
        .unwrap();
    plain
        .update_settings(SettingsSnapshot {
            bt_trackers: vec![Url::parse("https://tracker.example/private/announce").unwrap()],
            ..SettingsSnapshot::default()
        })
        .await
        .unwrap();
    drop(plain);

    let secrets = Arc::new(MemorySecretStore::default());
    let store = SqliteTaskStore::connect_with_secrets(&path, secrets.clone())
        .await
        .unwrap();

    let raw_task: String = sqlx::query_scalar("SELECT task_json FROM tasks WHERE id = ?1")
        .bind(detail.task.id.to_string())
        .fetch_one(store.pool())
        .await
        .unwrap();
    let raw_credentials: String =
        sqlx::query_scalar("SELECT credentials_json FROM tasks WHERE id = ?1")
            .bind(detail.task.id.to_string())
            .fetch_one(store.pool())
            .await
            .unwrap();
    let raw_settings: String =
        sqlx::query_scalar("SELECT value_json FROM settings WHERE key = 'global'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert!(!raw_task.contains("top-secret"));
    assert!(!raw_credentials.contains("top-secret"));
    assert!(!raw_settings.contains("tracker.example"));

    let migrated = store.get_task(detail.task.id).await.unwrap().unwrap();
    assert_eq!(
        migrated.credentials.extra.get(SECRET_HTTP_SOURCE_URL),
        Some(&signed_url.to_string())
    );
    assert_eq!(store.get_settings().await.unwrap().bt_trackers.len(), 1);
    assert!(secrets.0.lock().unwrap().len() >= 2);
}

#[tokio::test]
async fn missing_secret_reference_is_not_silently_replaced() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("test.sqlite");
    let plain = SqliteTaskStore::connect(&path).await.unwrap();
    let detail = plain
        .insert_task(CreateTaskInput {
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
    sqlx::query("UPDATE tasks SET credentials_json = ?2 WHERE id = ?1")
        .bind(detail.task.id.to_string())
        .bind(r#"{"__secret_ref":"missing"}"#)
        .execute(plain.pool())
        .await
        .unwrap();
    drop(plain);

    let result =
        SqliteTaskStore::connect_with_secrets(&path, Arc::new(MemorySecretStore::default())).await;
    assert!(result.is_err());

    let reopened = SqliteTaskStore::connect(&path).await.unwrap();
    let raw: String = sqlx::query_scalar("SELECT credentials_json FROM tasks WHERE id = ?1")
        .bind(detail.task.id.to_string())
        .fetch_one(reopened.pool())
        .await
        .unwrap();
    assert!(raw.contains("missing"));
}

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
