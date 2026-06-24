use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use fluxion_core::{
    DownloadEngine, DownloadKind, EngineContext, EngineExit, FluxionError, FluxionErrorKind,
    HeaderPair, HttpResourceMeta, HttpSegment, HttpSegmentState, HttpTaskConfig, PreparedTask,
    Result, TaskControl, TaskId, TaskKind, TaskState,
};
use futures::StreamExt;
use reqwest::{Client, header::RANGE};
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::{
    headers::build_headers,
    planner::plan_segments,
    probe::{ResourceProbe, http_status_error, network_error, probe},
    writer::PositionedWriter,
};

pub struct HttpEngine {
    client: Client,
}

const SEGMENT_PROGRESS_PERSIST_INTERVAL: u64 = 1024 * 1024;

impl HttpEngine {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(
                HttpTaskConfig::DEFAULT_REDIRECT_LIMIT as usize,
            ))
            .build()
            .map_err(network_error)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl DownloadEngine for HttpEngine {
    fn kind(&self) -> DownloadKind {
        DownloadKind::Http
    }

    async fn prepare(&self, ctx: EngineContext, mut task: PreparedTask) -> Result<PreparedTask> {
        let TaskKind::Http(config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an HTTP task",
            ));
        };
        let mut headers = config.headers.clone();
        headers.extend(task.credentials.headers.clone());
        let previous_meta = ctx.storage.get_http_meta(task.task.id).await?;
        let resource = probe(&self.client, config, &headers).await?;
        if has_http_resume_state(&ctx, task.task.id, previous_meta.as_ref()).await? {
            validate_resume_meta(previous_meta.as_ref(), &resource)?;
        }
        let meta = HttpResourceMeta {
            final_url: resource.final_url.clone(),
            etag: resource.etag.clone(),
            last_modified: resource.last_modified.clone(),
            content_length: resource.total_bytes,
            supports_ranges: resource.supports_ranges,
            temp_path: previous_meta.and_then(|meta| meta.temp_path),
        };
        ctx.storage.update_http_meta(task.task.id, meta).await?;
        task.task.total_bytes = resource.total_bytes;
        if !resource.supports_ranges {
            let mut config = config.clone();
            config.max_connections = Some(1);
            task.task.kind = TaskKind::Http(config);
        }
        if task.task.file_name.is_none() {
            task.task.file_name = resource.file_name.or_else(|| {
                resource
                    .final_url
                    .path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            });
        }
        ctx.storage.update_task(task.task.clone()).await?;
        Ok(task)
    }

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit> {
        let TaskKind::Http(config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an HTTP task",
            ));
        };
        let file_name = task
            .task
            .file_name
            .clone()
            .unwrap_or_else(|| "download".to_string());
        tokio::fs::create_dir_all(&task.task.save_dir).await?;
        let output_path = task.task.save_dir.join(sanitize_file_name(&file_name));
        let part_path = part_path(&output_path);

        let mut headers = config.headers.clone();
        headers.extend(task.credentials.headers.clone());
        let total = task.task.total_bytes;
        let max_connections = config
            .max_connections
            .unwrap_or(HttpTaskConfig::DEFAULT_MAX_CONNECTIONS);
        let min_split_size = config
            .min_split_size
            .unwrap_or(HttpTaskConfig::DEFAULT_MIN_SPLIT_SIZE);
        let segmented = total
            .map(|total| total > min_split_size && max_connections > 1)
            .unwrap_or(false);
        let existing_meta = ctx.storage.get_http_meta(task.task.id).await?;
        ctx.storage
            .update_http_meta(
                task.task.id,
                HttpResourceMeta {
                    final_url: existing_meta
                        .as_ref()
                        .map(|meta| meta.final_url.clone())
                        .unwrap_or_else(|| config.url.clone()),
                    etag: existing_meta.as_ref().and_then(|meta| meta.etag.clone()),
                    last_modified: existing_meta
                        .as_ref()
                        .and_then(|meta| meta.last_modified.clone()),
                    content_length: task
                        .task
                        .total_bytes
                        .or_else(|| existing_meta.as_ref().and_then(|meta| meta.content_length)),
                    supports_ranges: existing_meta
                        .as_ref()
                        .map(|meta| meta.supports_ranges)
                        .unwrap_or(segmented),
                    temp_path: Some(part_path.clone()),
                },
            )
            .await?;

        if segmented {
            let total = total.unwrap();
            let segments = self
                .segments_for_task(&ctx, task.task.id, total, max_connections, min_split_size)
                .await?;
            debug!(
                segments = segments.len(),
                "starting segmented HTTP download"
            );
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&part_path)
                .await?;
            file.set_len(total).await?;
            drop(file);
            self.download_segmented(
                &ctx,
                &task,
                config,
                &headers,
                control.clone(),
                &part_path,
                segments,
                total,
            )
            .await?;
        } else {
            self.download_single(&ctx, &task, config, &headers, control.clone(), &part_path)
                .await?;
        }

        if control.is_cancelled() {
            return Err(FluxionError::cancelled());
        }
        validate_size(&part_path, task.task.total_bytes).await?;
        tokio::fs::rename(&part_path, &output_path).await?;
        Ok(EngineExit::Completed {
            file_path: Some(output_path),
        })
    }
}

impl HttpEngine {
    async fn download_single(
        &self,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &HttpTaskConfig,
        headers: &[HeaderPair],
        control: TaskControl,
        part_path: &Path,
    ) -> Result<()> {
        let existing = existing_len(part_path).await?;
        let mut request = self
            .client
            .get(config.url.clone())
            .headers(build_headers(headers)?);
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let response = request.send().await.map_err(network_error)?;
        if existing > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(FluxionError::new(
                FluxionErrorKind::RangeNotSupported,
                "server did not resume partial single-thread download",
            ));
        }
        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(http_status_error(response.status()));
        }
        let total = task
            .task
            .total_bytes
            .or_else(|| response.content_length().map(|len| len + existing));
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(part_path)
            .await?;
        let mut downloaded = existing;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if control.is_cancelled() {
                file.flush().await?;
                return Err(FluxionError::cancelled());
            }
            let chunk = chunk.map_err(network_error)?;
            control.acquire_download(chunk.len() as u64).await;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            throttled_progress(ctx, task.task.id, downloaded, 0, total, false).await?;
        }
        file.flush().await?;
        throttled_progress(ctx, task.task.id, downloaded, 0, total, true).await?;
        Ok(())
    }

    async fn segments_for_task(
        &self,
        ctx: &EngineContext,
        task_id: TaskId,
        total: u64,
        max_connections: u16,
        min_split_size: u64,
    ) -> Result<Vec<HttpSegment>> {
        let existing = ctx.storage.list_http_segments(task_id).await?;
        if !existing.is_empty()
            && existing
                .last()
                .map(|segment| segment.end_byte + 1 == total)
                .unwrap_or(false)
        {
            return Ok(existing);
        }
        let segments = plan_segments(total, max_connections, min_split_size)
            .into_iter()
            .map(|plan| HttpSegment {
                index: plan.index as u32,
                start_byte: plan.start,
                end_byte: plan.end,
                downloaded_bytes: 0,
                state: HttpSegmentState::Pending,
                retry_count: 0,
                last_error: None,
            })
            .collect::<Vec<_>>();
        ctx.storage
            .replace_http_segments(task_id, segments.clone())
            .await?;
        Ok(segments)
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_segmented(
        &self,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &HttpTaskConfig,
        headers: &[HeaderPair],
        control: TaskControl,
        part_path: &Path,
        segments: Vec<HttpSegment>,
        total: u64,
    ) -> Result<()> {
        let writer = Arc::new(PositionedWriter::open(part_path).await?);
        let completed = segments
            .iter()
            .map(|segment| segment.downloaded_bytes)
            .sum::<u64>();
        let downloaded = Arc::new(tokio::sync::Mutex::new(completed));
        let mut handles = Vec::new();
        for segment in segments {
            if segment.is_complete() {
                continue;
            }
            let client = self.client.clone();
            let config = config.clone();
            let headers = headers.to_vec();
            let writer = writer.clone();
            let control = control.clone();
            let ctx = ctx.clone();
            let task_id = task.task.id;
            let downloaded = downloaded.clone();
            handles.push(tokio::spawn(async move {
                let mut attempts = 0_u8;
                let mut segment = segment;
                loop {
                    attempts += 1;
                    match download_segment_once(
                        &client,
                        &config,
                        &headers,
                        writer.clone(),
                        control.clone(),
                        ctx.clone(),
                        task_id,
                        downloaded.clone(),
                        total,
                        segment.clone(),
                    )
                    .await
                    {
                        Ok(()) => return Ok(()),
                        Err(error) if attempts < 5 && is_retryable(&error) => {
                            if let Ok(segments) = ctx.storage.list_http_segments(task_id).await
                                && let Some(stored) = segments
                                    .into_iter()
                                    .find(|item| item.index == segment.index)
                            {
                                segment = stored;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(
                                200 * u64::from(attempts),
                            ))
                            .await;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }));
        }
        for handle in handles {
            handle.await.map_err(|error| {
                FluxionError::new(FluxionErrorKind::Unknown, error.to_string())
            })??;
        }
        writer.flush().await?;
        throttled_progress(ctx, task.task.id, total, 0, Some(total), true).await?;
        ctx.set_state(task.task.id, TaskState::Verifying).await?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn download_segment_once(
    client: &Client,
    config: &HttpTaskConfig,
    headers: &[HeaderPair],
    writer: Arc<PositionedWriter>,
    control: TaskControl,
    ctx: EngineContext,
    task_id: TaskId,
    downloaded: Arc<tokio::sync::Mutex<u64>>,
    total: u64,
    mut segment: HttpSegment,
) -> Result<()> {
    if segment.downloaded_bytes > segment.end_byte - segment.start_byte + 1 {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            "stored segment progress exceeds segment size",
        ));
    }
    let range_start = segment.next_offset();
    if range_start > segment.end_byte {
        segment.state = HttpSegmentState::Completed;
        ctx.storage.update_http_segment(task_id, segment).await?;
        return Ok(());
    }
    segment.state = HttpSegmentState::Downloading;
    ctx.storage
        .update_http_segment(task_id, segment.clone())
        .await?;
    let mut last_persisted = segment.downloaded_bytes;
    let response = client
        .get(config.url.clone())
        .headers(build_headers(headers)?)
        .header(RANGE, format!("bytes={}-{}", range_start, segment.end_byte))
        .send()
        .await
        .map_err(network_error)?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(http_status_error(response.status()));
    }
    let mut offset = range_start;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if control.is_cancelled() {
            ctx.storage
                .update_http_segment(task_id, segment.clone())
                .await?;
            return Err(FluxionError::cancelled());
        }
        let chunk = chunk.map_err(network_error)?;
        control.acquire_download(chunk.len() as u64).await;
        writer.write_at(offset, &chunk).await?;
        offset += chunk.len() as u64;
        segment.downloaded_bytes += chunk.len() as u64;
        if segment.downloaded_bytes.saturating_sub(last_persisted)
            >= SEGMENT_PROGRESS_PERSIST_INTERVAL
        {
            ctx.storage
                .update_http_segment(task_id, segment.clone())
                .await?;
            last_persisted = segment.downloaded_bytes;
        }
        let current = {
            let mut downloaded = downloaded.lock().await;
            *downloaded += chunk.len() as u64;
            *downloaded
        };
        throttled_progress(&ctx, task_id, current, 0, Some(total), false).await?;
    }
    segment.state = HttpSegmentState::Completed;
    ctx.storage.update_http_segment(task_id, segment).await?;
    Ok(())
}

async fn has_http_resume_state(
    ctx: &EngineContext,
    task_id: TaskId,
    previous_meta: Option<&HttpResourceMeta>,
) -> Result<bool> {
    let segments = ctx.storage.list_http_segments(task_id).await?;
    if segments.iter().any(|segment| {
        segment.downloaded_bytes > 0
            || matches!(
                segment.state,
                HttpSegmentState::Downloading | HttpSegmentState::Completed
            )
    }) {
        return Ok(true);
    }
    let Some(temp_path) = previous_meta.and_then(|meta| meta.temp_path.as_ref()) else {
        return Ok(false);
    };
    Ok(existing_len(temp_path).await? > 0 && segments.is_empty())
}

fn validate_resume_meta(
    previous: Option<&HttpResourceMeta>,
    current: &ResourceProbe,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous.final_url != current.final_url {
        return Err(validator_mismatch("final URL changed"));
    }
    if let (Some(previous), Some(current)) = (&previous.etag, &current.etag)
        && previous != current
    {
        return Err(validator_mismatch("ETag changed"));
    }
    if let (Some(previous), Some(current)) = (&previous.last_modified, &current.last_modified)
        && previous != current
    {
        return Err(validator_mismatch("Last-Modified changed"));
    }
    if let (Some(previous), Some(current)) = (previous.content_length, current.total_bytes)
        && previous != current
    {
        return Err(validator_mismatch("content length changed"));
    }
    if previous.supports_ranges != current.supports_ranges {
        return Err(validator_mismatch("Range support changed"));
    }
    Ok(())
}

fn validator_mismatch(reason: &str) -> FluxionError {
    FluxionError::new(
        FluxionErrorKind::RangeNotSupported,
        format!("cannot safely resume HTTP task: {reason}"),
    )
}

fn is_retryable(error: &FluxionError) -> bool {
    matches!(
        error.kind(),
        FluxionErrorKind::Network | FluxionErrorKind::HttpStatus | FluxionErrorKind::Unknown
    )
}

fn sanitize_file_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ => ch,
        })
        .collect()
}

fn part_path(path: &std::path::Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".fluxionpart");
    PathBuf::from(os)
}

async fn existing_len(path: &Path) -> Result<u64> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

async fn validate_size(path: &Path, expected: Option<u64>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = tokio::fs::metadata(path).await?.len();
    if actual != expected {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            format!("downloaded file size mismatch: expected {expected}, got {actual}"),
        ));
    }
    Ok(())
}

async fn throttled_progress(
    ctx: &EngineContext,
    task_id: TaskId,
    downloaded_bytes: u64,
    uploaded_bytes: u64,
    total_bytes: Option<u64>,
    force: bool,
) -> Result<()> {
    if force || downloaded_bytes % (1024 * 1024) < 256 * 1024 {
        ctx.progress(task_id, downloaded_bytes, uploaded_bytes, total_bytes)
            .await?;
    }
    Ok(())
}
