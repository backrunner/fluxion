use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use fluxion_core::{
    DownloadEngine, DownloadKind, EngineContext, EngineExit, FluxionError, FluxionErrorKind,
    HeaderPair, HttpResourceMeta, HttpSegment, HttpSegmentState, HttpTaskConfig, PreparedTask,
    ProxyConfig, ProxyPolicy, Result, SECRET_HTTP_SOURCE_URL, SECRET_PROXY_URL, TaskControl,
    TaskId, TaskKind, TaskState,
};
use futures::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{CONTENT_RANGE, IF_RANGE, RANGE},
};
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::{
    headers::build_headers,
    planner::plan_segments,
    probe::{ResourceProbe, http_status_error, network_error, parse_content_range, probe},
    writer::PositionedWriter,
};

pub struct HttpEngine {
    client: Client,
}

const SEGMENT_PROGRESS_PERSIST_INTERVAL: u64 = 1024 * 1024;
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(300);
const MAX_RETRIES: u8 = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_TIMEOUT: Duration = Duration::from_secs(60);

impl HttpEngine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: build_client(HttpTaskConfig::DEFAULT_REDIRECT_LIMIT, None)?,
        })
    }

    /// Resolve the client for one task: honours the task's proxy policy (and
    /// the global "use system proxy" setting) plus its redirect limit. The
    /// shared default client is reused on the common direct/default path.
    async fn client_for(
        &self,
        ctx: &EngineContext,
        config: &HttpTaskConfig,
        policy: &ProxyPolicy,
    ) -> Result<Client> {
        let proxy = resolve_proxy(ctx, policy).await;
        if proxy.is_none() && config.redirect_limit == HttpTaskConfig::DEFAULT_REDIRECT_LIMIT {
            return Ok(self.client.clone());
        }
        build_client(config.redirect_limit, proxy.as_ref())
    }
}

fn effective_http_config(
    stored: &HttpTaskConfig,
    credentials: &fluxion_core::TaskCredentials,
) -> Result<HttpTaskConfig> {
    let mut config = stored.clone();
    if let Some(url) = credentials.extra.get(SECRET_HTTP_SOURCE_URL) {
        config.url = url::Url::parse(url).map_err(|error| {
            FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                format!("stored HTTP source URL is invalid: {error}"),
            )
        })?;
    }
    Ok(config)
}

fn effective_proxy_policy(
    stored: &ProxyPolicy,
    credentials: &fluxion_core::TaskCredentials,
) -> Result<ProxyPolicy> {
    let mut policy = stored.clone();
    if let (ProxyPolicy::Custom(config), Some(url)) =
        (&mut policy, credentials.extra.get(SECRET_PROXY_URL))
    {
        config.url = url::Url::parse(url).map_err(|error| {
            FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                format!("stored proxy URL is invalid: {error}"),
            )
        })?;
    }
    Ok(policy)
}

async fn resolve_proxy(ctx: &EngineContext, policy: &ProxyPolicy) -> Option<ProxyConfig> {
    match policy {
        ProxyPolicy::Direct => None,
        ProxyPolicy::Custom(config) => Some(config.clone()),
        ProxyPolicy::System => fluxion_platform::system_proxy().await.ok().flatten(),
        ProxyPolicy::UseGlobal => {
            let use_system = ctx
                .storage
                .get_settings()
                .await
                .map(|settings| settings.use_system_proxy)
                .unwrap_or(true);
            if use_system {
                fluxion_platform::system_proxy().await.ok().flatten()
            } else {
                None
            }
        }
    }
}

fn build_client(redirect_limit: u8, proxy: Option<&ProxyConfig>) -> Result<Client> {
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(redirect_limit as usize))
        // Without these a black-holed connection pends forever and the
        // cancellation flag is never rechecked — pause/stop would hang.
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT);
    builder = match proxy {
        Some(config) => {
            builder.proxy(reqwest::Proxy::all(config.url.as_str()).map_err(network_error)?)
        }
        // Explicit direct: also ignore ambient env-var proxies.
        None => builder.no_proxy(),
    };
    builder.build().map_err(network_error)
}

/// Time-based progress-event throttle shared by all workers of one task
/// (design §3.4: aggregate progress every 250–500 ms).
#[derive(Clone)]
struct ProgressThrottle {
    last: Arc<std::sync::Mutex<Instant>>,
}

impl ProgressThrottle {
    fn new() -> Self {
        Self {
            last: Arc::new(std::sync::Mutex::new(
                Instant::now() - PROGRESS_EMIT_INTERVAL,
            )),
        }
    }

    fn ready(&self) -> bool {
        let mut last = self.last.lock().expect("progress throttle poisoned");
        if last.elapsed() >= PROGRESS_EMIT_INTERVAL {
            *last = Instant::now();
            true
        } else {
            false
        }
    }
}

async fn emit_progress(
    ctx: &EngineContext,
    throttle: &ProgressThrottle,
    task_id: TaskId,
    downloaded_bytes: u64,
    uploaded_bytes: u64,
    total_bytes: Option<u64>,
    force: bool,
) {
    if force || throttle.ready() {
        // A failed progress write must not kill the download.
        if let Err(error) = ctx
            .progress(task_id, downloaded_bytes, uploaded_bytes, total_bytes)
            .await
        {
            warn!(?task_id, ?error, "failed to persist progress");
        }
    }
}

/// Await a future, bailing out with `Cancelled` as soon as the task's token
/// fires — network waits must never outlive a pause/stop request.
async fn cancellable<T>(
    control: &TaskControl,
    fut: impl std::future::Future<Output = T>,
) -> Result<T> {
    let token = control.cancellation_token();
    tokio::select! {
        result = fut => Ok(result),
        _ = token.cancelled() => Err(FluxionError::cancelled()),
    }
}

fn retry_backoff(attempt: u8) -> Duration {
    // Exponential: 250ms, 500ms, 1s, 2s, 4s … capped at 10s.
    Duration::from_millis(250u64.saturating_mul(1 << u32::from(attempt.saturating_sub(1)).min(6)))
        .min(Duration::from_secs(10))
}

#[async_trait]
impl DownloadEngine for HttpEngine {
    fn kind(&self) -> DownloadKind {
        DownloadKind::Http
    }

    async fn prepare(&self, ctx: EngineContext, mut task: PreparedTask) -> Result<PreparedTask> {
        let TaskKind::Http(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an HTTP task",
            ));
        };
        let config = effective_http_config(stored_config, &task.credentials)?;
        let proxy = effective_proxy_policy(&task.task.proxy, &task.credentials)?;
        let mut headers = config.headers.clone();
        headers.extend(task.credentials.headers.clone());
        let mut previous_meta = ctx.storage.get_http_meta(task.task.id).await?;
        let client = self.client_for(&ctx, &config, &proxy).await?;
        let resource = probe(&client, &config, &headers).await?;
        if has_http_resume_state(&ctx, task.task.id, previous_meta.as_ref()).await? {
            // Requirements §4.2: when stored validators no longer match (or no
            // validator exists at all), the resume data cannot be trusted —
            // discard it and restart from zero instead of failing forever.
            let verdict = match validate_resume_meta(previous_meta.as_ref(), &resource) {
                Ok(()) if resume_has_validators(previous_meta.as_ref(), &resource) => Ok(()),
                Ok(()) => Err("no ETag/Last-Modified validator available".to_string()),
                Err(error) => Err(error.to_string()),
            };
            if let Err(reason) = verdict {
                warn!(task_id = ?task.task.id, %reason, "resume state invalid; restarting from zero");
                ctx.storage
                    .replace_http_segments(task.task.id, Vec::new())
                    .await?;
                if let Some(part) = previous_meta
                    .as_ref()
                    .and_then(|meta| meta.temp_path.clone())
                {
                    let _ = tokio::fs::remove_file(&part).await;
                }
                previous_meta = None;
                let _ = ctx.progress(task.task.id, 0, 0, resource.total_bytes).await;
            }
        }
        let meta = HttpResourceMeta {
            final_url: fluxion_core::redact_url(&resource.final_url),
            etag: resource.etag.clone(),
            last_modified: resource.last_modified.clone(),
            content_length: resource.total_bytes,
            supports_ranges: resource.supports_ranges,
            temp_path: previous_meta.and_then(|meta| meta.temp_path),
        };
        ctx.storage.update_http_meta(task.task.id, meta).await?;
        task.task.total_bytes = resource.total_bytes;
        if !resource.supports_ranges
            && let TaskKind::Http(config) = &mut task.task.kind
        {
            config.max_connections = Some(1);
        }
        if task.task.file_name.is_none() {
            // Persist the decoded + sanitized name so the database always
            // matches the file the engine writes to disk (open/reveal/delete
            // resolve paths from the stored name).
            task.task.file_name = resource
                .file_name
                .or_else(|| {
                    resource
                        .final_url
                        .path_segments()
                        .and_then(|mut segments| segments.next_back())
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .map(|name| {
                    fluxion_core::sanitize_file_name(&fluxion_core::percent_decode_lossy(&name))
                });
        } else if let Some(name) = &task.task.file_name {
            task.task.file_name = Some(fluxion_core::sanitize_file_name(name));
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
        let TaskKind::Http(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not an HTTP task",
            ));
        };
        let config = effective_http_config(stored_config, &task.credentials)?;
        let proxy = effective_proxy_policy(&task.task.proxy, &task.credentials)?;
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
            .unwrap_or(HttpTaskConfig::DEFAULT_MIN_SPLIT_SIZE)
            .max(1);
        let segmented = total
            .map(|total| total > min_split_size && max_connections > 1)
            .unwrap_or(false);
        let existing_meta = ctx.storage.get_http_meta(task.task.id).await?;
        let resume_validator = existing_meta.as_ref().and_then(http_resume_validator);
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

        let throttle = ProgressThrottle::new();
        let client = self.client_for(&ctx, &config, &proxy).await?;
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
                &client,
                &ctx,
                &task,
                &config,
                &headers,
                control.clone(),
                &part_path,
                segments,
                total,
                resume_validator.clone(),
                throttle,
            )
            .await?;
        } else {
            // Single-connection path with its own retry loop: the download is
            // resumable (append + Range from the existing length), so each
            // attempt continues where the previous one ended.
            let mut attempts = 0_u8;
            loop {
                attempts += 1;
                match self
                    .download_single(
                        &client,
                        &ctx,
                        &task,
                        &config,
                        &headers,
                        control.clone(),
                        &part_path,
                        resume_validator.as_deref(),
                        &throttle,
                    )
                    .await
                {
                    Ok(()) => break,
                    Err(error)
                        if attempts < MAX_RETRIES
                            && is_retryable(&error)
                            && !control.is_cancelled() =>
                    {
                        warn!(task_id = ?task.task.id, attempt = attempts, %error, "single download failed; retrying");
                        tokio::time::sleep(retry_backoff(attempts)).await;
                    }
                    Err(error) => return Err(error),
                }
            }
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
    #[allow(clippy::too_many_arguments)]
    async fn download_single(
        &self,
        client: &Client,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &HttpTaskConfig,
        headers: &[HeaderPair],
        control: TaskControl,
        part_path: &Path,
        resume_validator: Option<&str>,
        throttle: &ProgressThrottle,
    ) -> Result<()> {
        let mut existing = existing_len(part_path).await?;
        let mut request = client
            .get(config.url.clone())
            .headers(build_headers(headers)?);
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
            if let Some(validator) = resume_validator {
                request = request.header(IF_RANGE, validator);
            }
        }
        let response = cancellable(&control, request.send())
            .await?
            .map_err(network_error)?;
        if existing > 0
            && response.status() == StatusCode::RANGE_NOT_SATISFIABLE
            && task.task.total_bytes == Some(existing)
        {
            emit_progress(
                ctx,
                throttle,
                task.task.id,
                existing,
                0,
                task.task.total_bytes,
                true,
            )
            .await;
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(http_status_error(response.status()));
        }
        if existing > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
            // Server ignored the Range header (200 with the full body).
            // Restart from zero rather than failing the task permanently.
            warn!(task_id = ?task.task.id, "server ignored resume Range; restarting single download from zero");
            tokio::fs::File::create(part_path).await?; // truncate
            existing = 0;
        } else if existing > 0 {
            validate_resume_content_range(
                response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok()),
                existing,
                task.task.total_bytes,
            )?;
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
        loop {
            let Some(chunk) = cancellable(&control, stream.next()).await? else {
                break;
            };
            if control.is_cancelled() {
                file.flush().await?;
                return Err(FluxionError::cancelled());
            }
            let chunk = chunk.map_err(network_error)?;
            control.acquire_download(chunk.len() as u64).await;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            emit_progress(ctx, throttle, task.task.id, downloaded, 0, total, false).await;
        }
        file.flush().await?;
        if let Some(total) = total
            && downloaded < total
        {
            return Err(FluxionError::new(
                FluxionErrorKind::Network,
                format!("connection closed early: got {downloaded} of {total} bytes"),
            ));
        }
        emit_progress(ctx, throttle, task.task.id, downloaded, 0, total, true).await;
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
        client: &Client,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &HttpTaskConfig,
        headers: &[HeaderPair],
        control: TaskControl,
        part_path: &Path,
        segments: Vec<HttpSegment>,
        total: u64,
        resume_validator: Option<String>,
        throttle: ProgressThrottle,
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
            let client = client.clone();
            let config = config.clone();
            let headers = headers.to_vec();
            let writer = writer.clone();
            let control = control.clone();
            let ctx = ctx.clone();
            let task_id = task.task.id;
            let downloaded = downloaded.clone();
            let resume_validator = resume_validator.clone();
            let throttle = throttle.clone();
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
                        resume_validator.as_deref(),
                        &mut segment,
                        &throttle,
                    )
                    .await
                    {
                        Ok(()) => return Ok(()),
                        Err(error)
                            if attempts < MAX_RETRIES
                                && is_retryable(&error)
                                && !control.is_cancelled() =>
                        {
                            // Reload the persisted state and roll the shared
                            // task counter back by whatever this attempt
                            // counted beyond the persisted offset — otherwise
                            // re-downloaded bytes are counted twice and the
                            // progress can exceed 100%.
                            let stored = ctx
                                .storage
                                .list_http_segments(task_id)
                                .await
                                .ok()
                                .and_then(|items| {
                                    items.into_iter().find(|item| item.index == segment.index)
                                });
                            let persisted = stored
                                .as_ref()
                                .map(|item| item.downloaded_bytes)
                                .unwrap_or(0);
                            let rollback = segment.downloaded_bytes.saturating_sub(persisted);
                            if rollback > 0 {
                                let mut count = downloaded.lock().await;
                                *count = count.saturating_sub(rollback);
                            }
                            let mut next = stored.unwrap_or_else(|| {
                                let mut fallback = segment.clone();
                                fallback.downloaded_bytes = persisted;
                                fallback
                            });
                            next.retry_count = u32::from(attempts);
                            next.last_error = Some(error.to_string());
                            let _ = ctx
                                .storage
                                .update_http_segment(task_id, next.clone())
                                .await;
                            segment = next;
                            warn!(?task_id, segment = segment.index, attempt = attempts, %error, "segment failed; retrying");
                            tokio::time::sleep(retry_backoff(attempts)).await;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }));
        }
        // Await every worker. On the first failure, abort the rest and drain
        // them — no orphan worker may keep writing after the task fails.
        let mut first_error: Option<FluxionError> = None;
        let mut iter = handles.into_iter();
        while let Some(handle) = iter.next() {
            let outcome = match handle.await {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(join_error) => Some(FluxionError::new(
                    FluxionErrorKind::Unknown,
                    join_error.to_string(),
                )),
            };
            if let Some(error) = outcome {
                first_error = Some(error);
                let rest: Vec<_> = iter.collect();
                for handle in &rest {
                    handle.abort();
                }
                for handle in rest {
                    let _ = handle.await;
                }
                break;
            }
        }
        if let Some(error) = first_error {
            writer.flush().await?;
            return Err(error);
        }
        writer.flush().await?;
        emit_progress(ctx, &throttle, task.task.id, total, 0, Some(total), true).await;
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
    resume_validator: Option<&str>,
    segment: &mut HttpSegment,
    throttle: &ProgressThrottle,
) -> Result<()> {
    let segment_len = segment.end_byte - segment.start_byte + 1;
    if segment.downloaded_bytes > segment_len {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            "stored segment progress exceeds segment size",
        ));
    }
    let range_start = segment.next_offset();
    if range_start > segment.end_byte {
        segment.state = HttpSegmentState::Completed;
        ctx.storage
            .update_http_segment(task_id, segment.clone())
            .await?;
        return Ok(());
    }
    segment.state = HttpSegmentState::Downloading;
    ctx.storage
        .update_http_segment(task_id, segment.clone())
        .await?;
    let mut last_persisted = segment.downloaded_bytes;
    let mut request = client
        .get(config.url.clone())
        .headers(build_headers(headers)?)
        .header(RANGE, format!("bytes={}-{}", range_start, segment.end_byte));
    if let Some(validator) = resume_validator {
        request = request.header(IF_RANGE, validator);
    }
    let response = cancellable(&control, request.send())
        .await?
        .map_err(network_error)?;
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(http_status_error(response.status()));
    }
    // The 206 must cover exactly the requested range; a CDN answering with a
    // different window would otherwise be written to the wrong offsets and
    // silently corrupt the file.
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range);
    match content_range {
        Some((start, end, Some(response_total)))
            if start == range_start && end == segment.end_byte && response_total == total => {}
        other => {
            return Err(FluxionError::new(
                FluxionErrorKind::Network,
                format!(
                    "server returned mismatched Content-Range (requested bytes {}-{}, got {:?})",
                    range_start, segment.end_byte, other
                ),
            ));
        }
    }
    let mut offset = range_start;
    let mut stream = response.bytes_stream();
    loop {
        let next = match cancellable(&control, stream.next()).await {
            Ok(next) => next,
            Err(cancelled) => {
                // Persist progress before surfacing the cancellation.
                ctx.storage
                    .update_http_segment(task_id, segment.clone())
                    .await?;
                return Err(cancelled);
            }
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(network_error)?;
        // Never write past the segment boundary, even if the server sends
        // extra bytes — that would clobber a neighbouring segment's data.
        if segment.downloaded_bytes + chunk.len() as u64 > segment_len {
            ctx.storage
                .update_http_segment(task_id, segment.clone())
                .await?;
            return Err(FluxionError::new(
                FluxionErrorKind::Network,
                "server sent more bytes than the requested range",
            ));
        }
        control.acquire_download(chunk.len() as u64).await;
        if control.is_cancelled() {
            ctx.storage
                .update_http_segment(task_id, segment.clone())
                .await?;
            return Err(FluxionError::cancelled());
        }
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
        emit_progress(&ctx, throttle, task_id, current, 0, Some(total), false).await;
    }
    // A stream that ends cleanly but short (server clamped the range, file
    // shrank, proxy cut the body) must NOT mark the segment complete.
    if segment.downloaded_bytes != segment_len {
        ctx.storage
            .update_http_segment(task_id, segment.clone())
            .await?;
        return Err(FluxionError::new(
            FluxionErrorKind::Network,
            format!(
                "segment ended early: got {} of {} bytes",
                segment.downloaded_bytes, segment_len
            ),
        ));
    }
    segment.state = HttpSegmentState::Completed;
    ctx.storage
        .update_http_segment(task_id, segment.clone())
        .await?;
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

/// Whether at least one strong-ish validator (ETag or Last-Modified) exists on
/// both sides of the resume comparison. Without any validator, resuming risks
/// stitching together two different file versions of the same length.
fn resume_has_validators(previous: Option<&HttpResourceMeta>, current: &ResourceProbe) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    (previous.etag.is_some() && current.etag.is_some())
        || (previous.last_modified.is_some() && current.last_modified.is_some())
}

fn http_resume_validator(meta: &HttpResourceMeta) -> Option<String> {
    meta.etag.clone().or_else(|| meta.last_modified.clone())
}

fn validate_resume_content_range(
    value: Option<&str>,
    expected_start: u64,
    expected_total: Option<u64>,
) -> Result<()> {
    let Some((start, _, total)) = value.and_then(parse_content_range) else {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            "resume response is missing a valid Content-Range",
        ));
    };
    if start != expected_start || expected_total.is_some_and(|expected| total != Some(expected)) {
        return Err(FluxionError::new(
            FluxionErrorKind::RangeNotSupported,
            format!(
                "resume response range does not match local progress (expected start {expected_start} and total {expected_total:?}, got start {start} and total {total:?})"
            ),
        ));
    }
    Ok(())
}

fn validate_resume_meta(
    previous: Option<&HttpResourceMeta>,
    current: &ResourceProbe,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous.final_url != fluxion_core::redact_url(&current.final_url) {
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
    // Network failures and transient HTTP statuses (5xx/429 — permanent 4xx
    // are mapped to other kinds in `http_status_error`). Everything else,
    // including DiskFull/PermissionDenied/Unknown, fails fast.
    matches!(
        error.kind(),
        FluxionErrorKind::Network | FluxionErrorKind::HttpStatus
    )
}

fn sanitize_file_name(input: &str) -> String {
    fluxion_core::sanitize_file_name(input)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_content_range_must_match_local_offset_and_total() {
        assert!(
            validate_resume_content_range(Some("bytes 512-1023/1024"), 512, Some(1024)).is_ok()
        );
        assert!(validate_resume_content_range(Some("bytes 0-511/1024"), 512, Some(1024)).is_err());
        assert!(
            validate_resume_content_range(Some("bytes 512-1023/2048"), 512, Some(1024)).is_err()
        );
        assert!(validate_resume_content_range(None, 512, Some(1024)).is_err());
    }
}
