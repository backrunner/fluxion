use std::{path::Path, str::FromStr};

use async_trait::async_trait;
use chrono::Utc;
use fluxion_core::{
    CreateTaskInput, DownloadKind, DownloadTask, FluxionError, FluxionErrorKind, HttpResourceMeta,
    HttpSegment, HttpSegmentState, Result, SettingsSnapshot, TaskCredentials, TaskDetail,
    TaskFilter, TaskId, TaskKind, TaskState, TaskStore, TaskSummary, new_task_id,
};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

pub struct SqliteTaskStore {
    pool: SqlitePool,
}

impl SqliteTaskStore {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite://{}?mode=rwc",
            path.as_ref().display()
        ))
        .map_err(storage_error)?
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(storage_error)?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn migrate(&self) -> Result<()> {
        for statement in MIGRATIONS {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .map_err(storage_error)?;
        }
        Ok(())
    }
}

#[async_trait]
impl TaskStore for SqliteTaskStore {
    async fn insert_task(&self, input: CreateTaskInput) -> Result<TaskDetail> {
        let id = new_task_id();
        let now = Utc::now();
        let task = DownloadTask {
            id,
            kind: input.kind,
            save_dir: input.save_dir,
            file_name: input.file_name,
            state: TaskState::Queued,
            limits: input.limits,
            proxy: input.proxy,
            total_bytes: None,
            downloaded_bytes: 0,
            uploaded_bytes: 0,
            error: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        let kind = task.kind.download_kind();
        sqlx::query(
            r#"
            INSERT INTO tasks (
                id, kind, state, save_dir, file_name, task_json, credentials_json,
                total_bytes, downloaded_bytes, uploaded_bytes, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        )
        .bind(id.to_string())
        .bind(kind_to_str(kind))
        .bind(state_to_str(&task.state))
        .bind(task.save_dir.to_string_lossy().to_string())
        .bind(task.file_name.clone())
        .bind(serde_json::to_string(&task).map_err(storage_error)?)
        .bind(serde_json::to_string(&input.credentials).map_err(storage_error)?)
        .bind(task.total_bytes.map(|value| value as i64))
        .bind(task.downloaded_bytes as i64)
        .bind(task.uploaded_bytes as i64)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        insert_protocol_rows(&self.pool, &task).await?;
        Ok(TaskDetail {
            task,
            credentials: input.credentials,
        })
    }

    async fn get_task(&self, task_id: TaskId) -> Result<Option<TaskDetail>> {
        let Some(row) = sqlx::query("SELECT task_json, credentials_json FROM tasks WHERE id = ?1")
            .bind(task_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?
        else {
            return Ok(None);
        };
        let task: DownloadTask = serde_json::from_str(row.get::<String, _>("task_json").as_str())
            .map_err(storage_error)?;
        let credentials: TaskCredentials =
            serde_json::from_str(row.get::<String, _>("credentials_json").as_str())
                .map_err(storage_error)?;
        Ok(Some(TaskDetail { task, credentials }))
    }

    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<TaskSummary>> {
        let rows = sqlx::query("SELECT task_json FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        let mut tasks = Vec::new();
        for row in rows {
            let task: DownloadTask =
                serde_json::from_str(row.get::<String, _>("task_json").as_str())
                    .map_err(storage_error)?;
            if !filter.states.is_empty() && !filter.states.contains(&task.state) {
                continue;
            }
            if !filter.kinds.is_empty() && !filter.kinds.contains(&task.kind.download_kind()) {
                continue;
            }
            tasks.push(TaskSummary::from(&task));
        }
        Ok(tasks)
    }

    async fn update_state(&self, task_id: TaskId, state: TaskState) -> Result<()> {
        self.mutate_task(task_id, |task| {
            task.state = state.clone();
            task.updated_at = Utc::now();
            if task.state == TaskState::Completed {
                task.completed_at = Some(Utc::now());
            }
        })
        .await
    }

    async fn update_progress(
        &self,
        task_id: TaskId,
        downloaded_bytes: u64,
        uploaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<()> {
        self.mutate_task(task_id, |task| {
            task.downloaded_bytes = downloaded_bytes;
            task.uploaded_bytes = uploaded_bytes;
            if total_bytes.is_some() {
                task.total_bytes = total_bytes;
            }
            task.updated_at = Utc::now();
        })
        .await
    }

    async fn update_task(&self, task: DownloadTask) -> Result<()> {
        write_task(&self.pool, &task).await
    }

    async fn update_error(&self, task_id: TaskId, message: Option<String>) -> Result<()> {
        self.mutate_task(task_id, |task| {
            task.error = message.clone();
            task.updated_at = Utc::now();
        })
        .await
    }

    async fn delete_task(&self, task_id: TaskId) -> Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?1")
            .bind(task_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn output_paths(&self, task_id: TaskId) -> Result<Vec<std::path::PathBuf>> {
        let Some(detail) = self.get_task(task_id).await? else {
            return Ok(Vec::new());
        };
        let mut paths = Vec::new();
        if let Some(file_name) = detail.task.file_name.clone() {
            paths.push(detail.task.save_dir.join(file_name.clone()));
            paths.push(
                detail
                    .task
                    .save_dir
                    .join(format!("{file_name}.fluxionpart")),
            );
        }
        let rows = sqlx::query("SELECT temp_path FROM http_tasks WHERE task_id = ?1")
            .bind(task_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        for row in rows {
            if let Some(temp_path) = row.get::<Option<String>, _>("temp_path") {
                paths.push(temp_path.into());
            }
        }
        Ok(paths)
    }

    async fn get_credentials(&self, task_id: TaskId) -> Result<TaskCredentials> {
        Ok(self
            .get_task(task_id)
            .await?
            .map(|detail| detail.credentials)
            .unwrap_or_default())
    }

    async fn get_settings(&self) -> Result<SettingsSnapshot> {
        let Some(row) = sqlx::query("SELECT value_json FROM settings WHERE key = 'global'")
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_error)?
        else {
            return Ok(SettingsSnapshot::default());
        };
        serde_json::from_str(row.get::<String, _>("value_json").as_str()).map_err(storage_error)
    }

    async fn update_settings(&self, settings: SettingsSnapshot) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value_json) VALUES ('global', ?1)
            ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
            "#,
        )
        .bind(serde_json::to_string(&settings).map_err(storage_error)?)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn get_http_meta(&self, task_id: TaskId) -> Result<Option<HttpResourceMeta>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT final_url, original_url, etag, last_modified, content_length, supports_ranges, temp_path
            FROM http_tasks
            WHERE task_id = ?1
            "#,
        )
        .bind(task_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?
        else {
            return Ok(None);
        };
        let final_url = row
            .get::<Option<String>, _>("final_url")
            .or_else(|| Some(row.get::<String, _>("original_url")));
        let Some(final_url) = final_url else {
            return Ok(None);
        };
        Ok(Some(HttpResourceMeta {
            final_url: url::Url::parse(&final_url).map_err(storage_error)?,
            etag: row.get::<Option<String>, _>("etag"),
            last_modified: row.get::<Option<String>, _>("last_modified"),
            content_length: row
                .get::<Option<i64>, _>("content_length")
                .map(|value| value as u64),
            supports_ranges: row
                .get::<Option<bool>, _>("supports_ranges")
                .unwrap_or(false),
            temp_path: row
                .get::<Option<String>, _>("temp_path")
                .map(std::path::PathBuf::from),
        }))
    }

    async fn update_http_meta(&self, task_id: TaskId, meta: HttpResourceMeta) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO http_tasks (
                task_id, original_url, final_url, etag, last_modified,
                content_length, supports_ranges, temp_path, max_connections
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(task_id) DO UPDATE SET
                final_url = excluded.final_url,
                etag = excluded.etag,
                last_modified = excluded.last_modified,
                content_length = excluded.content_length,
                supports_ranges = excluded.supports_ranges,
                temp_path = excluded.temp_path,
                max_connections = excluded.max_connections
            "#,
        )
        .bind(task_id.to_string())
        .bind(meta.final_url.to_string())
        .bind(meta.final_url.to_string())
        .bind(meta.etag)
        .bind(meta.last_modified)
        .bind(meta.content_length.map(|value| value as i64))
        .bind(meta.supports_ranges)
        .bind(
            meta.temp_path
                .map(|path| path.to_string_lossy().to_string()),
        )
        .bind(None::<i64>)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }

    async fn replace_http_segments(
        &self,
        task_id: TaskId,
        segments: Vec<HttpSegment>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query("DELETE FROM http_segments WHERE task_id = ?1")
            .bind(task_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        for segment in segments {
            insert_http_segment(&mut tx, task_id, segment).await?;
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn list_http_segments(&self, task_id: TaskId) -> Result<Vec<HttpSegment>> {
        let rows = sqlx::query(
            r#"
            SELECT segment_index, start_byte, end_byte, downloaded_bytes, state, retry_count, last_error
            FROM http_segments
            WHERE task_id = ?1
            ORDER BY segment_index ASC
            "#,
        )
        .bind(task_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let mut segments = Vec::with_capacity(rows.len());
        for row in rows {
            segments.push(http_segment_from_row(row)?);
        }
        Ok(segments)
    }

    async fn update_http_segment(&self, task_id: TaskId, segment: HttpSegment) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO http_segments (
                task_id, segment_index, start_byte, end_byte, downloaded_bytes, state, retry_count, last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(task_id, segment_index) DO UPDATE SET
                start_byte = excluded.start_byte,
                end_byte = excluded.end_byte,
                downloaded_bytes = excluded.downloaded_bytes,
                state = excluded.state,
                retry_count = excluded.retry_count,
                last_error = excluded.last_error
            "#,
        )
        .bind(task_id.to_string())
        .bind(i64::from(segment.index))
        .bind(segment.start_byte as i64)
        .bind(segment.end_byte as i64)
        .bind(segment.downloaded_bytes as i64)
        .bind(http_segment_state_to_str(&segment.state))
        .bind(i64::from(segment.retry_count))
        .bind(segment.last_error)
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;
        Ok(())
    }
}

impl SqliteTaskStore {
    async fn mutate_task(
        &self,
        task_id: TaskId,
        mutate: impl FnOnce(&mut DownloadTask),
    ) -> Result<()> {
        let mut detail = self
            .get_task(task_id)
            .await?
            .ok_or_else(|| FluxionError::new(FluxionErrorKind::Storage, "task not found"))?;
        mutate(&mut detail.task);
        write_task(&self.pool, &detail.task).await
    }
}

async fn write_task(pool: &SqlitePool, task: &DownloadTask) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE tasks SET
            state = ?2,
            file_name = ?3,
            task_json = ?4,
            total_bytes = ?5,
            downloaded_bytes = ?6,
            uploaded_bytes = ?7,
            error = ?8,
            updated_at = ?9,
            completed_at = ?10
        WHERE id = ?1
        "#,
    )
    .bind(task.id.to_string())
    .bind(state_to_str(&task.state))
    .bind(task.file_name.clone())
    .bind(serde_json::to_string(task).map_err(storage_error)?)
    .bind(task.total_bytes.map(|value| value as i64))
    .bind(task.downloaded_bytes as i64)
    .bind(task.uploaded_bytes as i64)
    .bind(task.error.clone())
    .bind(task.updated_at.to_rfc3339())
    .bind(task.completed_at.map(|value| value.to_rfc3339()))
    .execute(pool)
    .await
    .map_err(storage_error)?;
    Ok(())
}

async fn insert_protocol_rows(pool: &SqlitePool, task: &DownloadTask) -> Result<()> {
    match &task.kind {
        TaskKind::Http(config) => {
            sqlx::query("INSERT INTO http_tasks (task_id, original_url, max_connections) VALUES (?1, ?2, ?3)")
                .bind(task.id.to_string())
                .bind(config.url.to_string())
                .bind(config.max_connections.map(i64::from))
                .execute(pool)
                .await
                .map_err(storage_error)?;
        }
        TaskKind::Bt(config) => {
            sqlx::query("INSERT INTO bt_tasks (task_id, source_json, max_connections, share_ratio_limit, enable_seeding) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(task.id.to_string())
                .bind(serde_json::to_string(&config.source).map_err(storage_error)?)
                .bind(config.max_connections.map(i64::from))
                .bind(config.share_ratio_limit)
                .bind(config.enable_seeding)
                .execute(pool)
                .await
                .map_err(storage_error)?;
            for tracker in &config.trackers {
                sqlx::query("INSERT INTO bt_trackers (task_id, url) VALUES (?1, ?2)")
                    .bind(task.id.to_string())
                    .bind(tracker.to_string())
                    .execute(pool)
                    .await
                    .map_err(storage_error)?;
            }
        }
        TaskKind::Ftp(config) => {
            sqlx::query("INSERT INTO ftp_tasks (task_id, url, username, passive, ftps) VALUES (?1, ?2, ?3, ?4, ?5)")
                .bind(task.id.to_string())
                .bind(config.url.to_string())
                .bind(config.username.clone())
                .bind(config.passive)
                .bind(config.ftps)
                .execute(pool)
                .await
                .map_err(storage_error)?;
        }
        TaskKind::Sftp(config) => {
            sqlx::query("INSERT INTO sftp_tasks (task_id, url, username, private_key_path) VALUES (?1, ?2, ?3, ?4)")
                .bind(task.id.to_string())
                .bind(config.url.to_string())
                .bind(config.username.clone())
                .bind(config.private_key_path.as_ref().map(|path| path.to_string_lossy().to_string()))
                .execute(pool)
                .await
                .map_err(storage_error)?;
        }
    }
    Ok(())
}

fn kind_to_str(kind: DownloadKind) -> &'static str {
    match kind {
        DownloadKind::Http => "http",
        DownloadKind::Bt => "bt",
        DownloadKind::Ftp => "ftp",
        DownloadKind::Sftp => "sftp",
    }
}

fn state_to_str(state: &TaskState) -> &'static str {
    match state {
        TaskState::Queued => "queued",
        TaskState::Resolving => "resolving",
        TaskState::Downloading => "downloading",
        TaskState::Paused => "paused",
        TaskState::Stopped => "stopped",
        TaskState::Completed => "completed",
        TaskState::Seeding => "seeding",
        TaskState::Failed => "failed",
        TaskState::Verifying => "verifying",
    }
}

fn http_segment_state_to_str(state: &HttpSegmentState) -> &'static str {
    match state {
        HttpSegmentState::Pending => "pending",
        HttpSegmentState::Downloading => "downloading",
        HttpSegmentState::Completed => "completed",
        HttpSegmentState::Failed => "failed",
    }
}

fn http_segment_from_row(row: sqlx::sqlite::SqliteRow) -> Result<HttpSegment> {
    let state = match row.get::<String, _>("state").as_str() {
        "pending" => HttpSegmentState::Pending,
        "downloading" => HttpSegmentState::Downloading,
        "completed" => HttpSegmentState::Completed,
        "failed" => HttpSegmentState::Failed,
        other => {
            return Err(FluxionError::new(
                FluxionErrorKind::Storage,
                format!("invalid segment state {other}"),
            ));
        }
    };
    Ok(HttpSegment {
        index: row.get::<i64, _>("segment_index") as u32,
        start_byte: row.get::<i64, _>("start_byte") as u64,
        end_byte: row.get::<i64, _>("end_byte") as u64,
        downloaded_bytes: row.get::<i64, _>("downloaded_bytes") as u64,
        state,
        retry_count: row.get::<i64, _>("retry_count") as u32,
        last_error: row.get::<Option<String>, _>("last_error"),
    })
}

async fn insert_http_segment(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: TaskId,
    segment: HttpSegment,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO http_segments (
            task_id, segment_index, start_byte, end_byte, downloaded_bytes, state, retry_count, last_error
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(task_id.to_string())
    .bind(i64::from(segment.index))
    .bind(segment.start_byte as i64)
    .bind(segment.end_byte as i64)
    .bind(segment.downloaded_bytes as i64)
    .bind(http_segment_state_to_str(&segment.state))
    .bind(i64::from(segment.retry_count))
    .bind(segment.last_error)
    .execute(&mut **tx)
    .await
    .map_err(storage_error)?;
    Ok(())
}

fn storage_error(error: impl std::error::Error + Send + Sync + 'static) -> FluxionError {
    FluxionError::new(FluxionErrorKind::Storage, error.to_string())
}

const MIGRATIONS: &[&str] = &[
    "PRAGMA journal_mode = WAL",
    "PRAGMA foreign_keys = ON",
    r#"
    CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        state TEXT NOT NULL,
        save_dir TEXT NOT NULL,
        file_name TEXT,
        task_json TEXT NOT NULL,
        credentials_json TEXT NOT NULL,
        total_bytes INTEGER,
        downloaded_bytes INTEGER NOT NULL DEFAULT 0,
        uploaded_bytes INTEGER NOT NULL DEFAULT 0,
        error TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_at TEXT
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS http_tasks (
        task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
        original_url TEXT NOT NULL,
        final_url TEXT,
        etag TEXT,
        last_modified TEXT,
        content_length INTEGER,
        supports_ranges INTEGER,
        max_connections INTEGER,
        temp_path TEXT
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS http_segments (
        task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
        segment_index INTEGER NOT NULL,
        start_byte INTEGER NOT NULL,
        end_byte INTEGER NOT NULL,
        downloaded_bytes INTEGER NOT NULL,
        state TEXT NOT NULL,
        retry_count INTEGER NOT NULL DEFAULT 0,
        last_error TEXT,
        PRIMARY KEY (task_id, segment_index)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS bt_tasks (
        task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
        source_json TEXT NOT NULL,
        max_connections INTEGER,
        share_ratio_limit REAL,
        enable_seeding INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS bt_files (
        task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
        file_index INTEGER NOT NULL,
        path TEXT NOT NULL,
        length INTEGER NOT NULL,
        downloaded_bytes INTEGER NOT NULL DEFAULT 0,
        selected INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (task_id, file_index)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS bt_pieces (
        task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
        piece_index INTEGER NOT NULL,
        state TEXT NOT NULL,
        downloaded_bytes INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (task_id, piece_index)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS bt_trackers (
        task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
        url TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS bt_ip_rules (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
        rule_type TEXT NOT NULL,
        cidr TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS ftp_tasks (
        task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
        url TEXT NOT NULL,
        username TEXT,
        passive INTEGER NOT NULL,
        ftps INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS sftp_tasks (
        task_id TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
        url TEXT NOT NULL,
        username TEXT,
        private_key_path TEXT
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL
    )
    "#,
];

pub fn parse_task_id(value: &str) -> Result<TaskId> {
    TaskId::from_str(value)
        .map_err(|error| FluxionError::new(FluxionErrorKind::InvalidConfig, error.to_string()))
}
