use async_trait::async_trait;
use fluxion_core::{
    AntiLeechConfig, BtFileState, BtSource, BtStateSnapshot, BtTaskConfig, DownloadEngine,
    DownloadKind, EngineContext, EngineExit, EngineStateProvider, FluxionError, FluxionErrorKind,
    IpFilterConfig, MagnetPreview, MagnetPreviewFile, PreparedTask, Result, SECRET_BT_MAGNET,
    SECRET_BT_TRACKER_PREFIX, TaskControl, TaskKind, TaskRateLimit, TaskState, apply_ip_filter,
};
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, Session, api::TorrentIdOrHash,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet, net::IpAddr, num::NonZeroU32, path::PathBuf, sync::Arc, time::Duration,
};
use tokio::sync::OnceCell;

/// Process-wide librqbit session. `Session` is designed as a per-process
/// singleton (it binds a listen port and runs DHT); creating one per task
/// causes port conflicts and leaks resources. Torrents are routed to their
/// destination via the per-torrent `output_folder` option instead.
static SHARED_SESSION: OnceCell<Arc<Session>> = OnceCell::const_new();

async fn shared_session() -> Result<&'static Arc<Session>> {
    SHARED_SESSION
        .get_or_try_init(|| async {
            // The session-level default output dir is never used: every
            // torrent (and magnet preview) passes an explicit output_folder.
            let base = std::env::temp_dir().join("fluxion-bt-session");
            tokio::fs::create_dir_all(&base).await?;
            Session::new(base).await.map_err(bt_error)
        })
        .await
}

pub struct BtEngine {
    adapter: RqbitAdapter,
}

impl BtEngine {
    pub fn new() -> Self {
        Self {
            adapter: RqbitAdapter,
        }
    }
}

impl Default for BtEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DownloadEngine for BtEngine {
    fn kind(&self) -> DownloadKind {
        DownloadKind::Bt
    }

    async fn prepare(&self, _ctx: EngineContext, task: PreparedTask) -> Result<PreparedTask> {
        let TaskKind::Bt(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not a BT task",
            ));
        };
        let config = effective_bt_config(stored_config, &task.credentials)?;
        validate_bt_source(&config.source)?;
        Ok(task)
    }

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit> {
        let TaskKind::Bt(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not a BT task",
            ));
        };
        let config = effective_bt_config(stored_config, &task.credentials)?;
        let handle = self
            .adapter
            .start(
                &config,
                task.task.save_dir.clone(),
                task.task.limits.clone(),
            )
            .await?;
        sync_session_rate_limits(&ctx, &handle).await?;
        // Register the live handle as a state provider so the UI can query
        // file/piece state while the task is running (or seeding).
        let provider: Arc<dyn EngineStateProvider> = Arc::new(handle.clone());
        ctx.register_state_provider(task.task.id, provider).await;
        loop {
            if let Err(error) = sync_session_rate_limits(&ctx, &handle).await {
                tracing::warn!(task_id = ?task.task.id, ?error, "failed to refresh BitTorrent global limits");
            }
            let stats = handle.stats();
            if let Some(error) = stats.error.clone() {
                return Err(FluxionError::new(FluxionErrorKind::Network, error));
            }
            ctx.progress(
                task.task.id,
                stats.progress_bytes,
                stats.uploaded_bytes,
                Some(stats.total_bytes),
            )
            .await?;
            if stats.finished {
                break;
            }
            if control.is_cancelled() {
                let _ = handle.remove().await;
                return Err(FluxionError::cancelled());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if config.enable_seeding {
            ctx.set_state(task.task.id, TaskState::Seeding).await?;
            // The download is already complete at this point, so both a
            // reached share ratio and a user cancellation of the seeding
            // phase end the task as Completed rather than as cancelled.
            self.seed_until_cancelled_or_ratio(&ctx, &task, &config, control, &handle)
                .await?;
        }
        let _ = handle.remove().await;
        Ok(EngineExit::Completed { file_path: None })
    }

    async fn resolve_magnet_preview(&self, magnet: &str) -> Result<MagnetPreview> {
        self.adapter.resolve_magnet_preview(magnet).await
    }
}

impl BtEngine {
    /// Seed until the configured share ratio is reached or the user cancels.
    /// Both outcomes return `Ok(())`: the download itself is already complete,
    /// so stopping the seeding phase must not be treated as a cancellation of
    /// the task.
    async fn seed_until_cancelled_or_ratio(
        &self,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &BtTaskConfig,
        control: TaskControl,
        handle: &RqbitDownload,
    ) -> Result<()> {
        loop {
            if let Err(error) = sync_session_rate_limits(ctx, handle).await {
                tracing::warn!(task_id = ?task.task.id, ?error, "failed to refresh BitTorrent global limits while seeding");
            }
            let stats = handle.stats();
            ctx.progress(
                task.task.id,
                stats.progress_bytes,
                stats.uploaded_bytes,
                Some(stats.total_bytes),
            )
            .await?;
            if let Some(limit) = config.share_ratio_limit
                && stats.total_bytes > 0
                && (stats.uploaded_bytes as f64 / stats.total_bytes as f64) >= limit
            {
                return Ok(());
            }
            if control.is_cancelled() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

async fn sync_session_rate_limits(ctx: &EngineContext, handle: &RqbitDownload) -> Result<()> {
    let settings = ctx.storage.get_settings().await?;
    handle
        .session
        .ratelimits
        .set_download_bps(nonzero_u32(settings.download_limit));
    handle
        .session
        .ratelimits
        .set_upload_bps(nonzero_u32(settings.upload_limit));
    Ok(())
}

fn validate_bt_source(source: &BtSource) -> Result<()> {
    match source {
        BtSource::TorrentFile(path) if path.as_os_str().is_empty() => Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "torrent path is empty",
        )),
        BtSource::Magnet(value) if !value.starts_with("magnet:?") => Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "invalid magnet link",
        )),
        _ => Ok(()),
    }
}

fn effective_bt_config(
    stored: &BtTaskConfig,
    credentials: &fluxion_core::TaskCredentials,
) -> Result<BtTaskConfig> {
    let mut config = stored.clone();
    if let Some(magnet) = credentials.extra.get(SECRET_BT_MAGNET) {
        config.source = BtSource::Magnet(magnet.clone());
    }
    let trackers = credentials
        .extra
        .iter()
        .filter(|(key, _)| key.starts_with(SECRET_BT_TRACKER_PREFIX))
        .map(|(_, value)| {
            value.parse().map_err(|error| {
                FluxionError::new(
                    FluxionErrorKind::InvalidConfig,
                    format!("stored BitTorrent tracker URL is invalid: {error}"),
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if !trackers.is_empty() {
        config.trackers = trackers;
    }
    Ok(config)
}

pub struct RqbitAdapter;

#[derive(Clone)]
pub struct RqbitDownload {
    session: Arc<Session>,
    handle: Arc<librqbit::ManagedTorrent>,
    selected_files: Option<HashSet<usize>>,
}

impl RqbitDownload {
    fn stats(&self) -> librqbit::TorrentStats {
        self.handle.stats()
    }

    /// Remove the torrent from the shared session, keeping downloaded files.
    /// Used on task exit/cancel so finished or stopped tasks do not keep
    /// occupying the process-wide session.
    async fn remove(&self) -> Result<()> {
        self.session
            .delete(TorrentIdOrHash::Id(self.handle.id()), false)
            .await
            .map_err(bt_error)
    }
}

#[async_trait]
impl EngineStateProvider for RqbitDownload {
    async fn snapshot(&self) -> Result<BtStateSnapshot> {
        let stats = self.handle.stats();
        // File list + per-file progress.
        let files = self
            .handle
            .metadata
            .load()
            .as_ref()
            .map(|meta| {
                meta.file_infos
                    .iter()
                    .enumerate()
                    .map(|(idx, info)| BtFileState {
                        index: idx as u32,
                        name: info.relative_filename.to_string_lossy().to_string(),
                        size: info.len,
                        downloaded: stats.file_progress.get(idx).copied().unwrap_or(0),
                        selected: self
                            .selected_files
                            .as_ref()
                            .is_none_or(|selected| selected.contains(&idx)),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Piece haves bitfield via the public Api.
        let api = Api::new(self.session.clone(), None);
        let (piece_haves, total_pieces) =
            match api.api_dump_haves(TorrentIdOrHash::Id(self.handle.shared().id)) {
                Ok((bf, len)) => (bf.as_raw_slice().to_vec(), len),
                Err(_) => (Vec::new(), 0),
            };

        let seed_ratio = if stats.total_bytes > 0 {
            Some(stats.uploaded_bytes as f64 / stats.total_bytes as f64)
        } else {
            None
        };

        Ok(BtStateSnapshot {
            files,
            piece_haves,
            total_pieces,
            seed_ratio,
        })
    }
}

impl RqbitAdapter {
    pub async fn start(
        &self,
        config: &BtTaskConfig,
        output_dir: PathBuf,
        limits: TaskRateLimit,
    ) -> Result<RqbitDownload> {
        tokio::fs::create_dir_all(&output_dir).await?;
        let session = shared_session().await?.clone();
        let add = match &config.source {
            BtSource::Magnet(value) => AddTorrent::from_url(value.clone()),
            BtSource::TorrentFile(path) => {
                let bytes = tokio::fs::read(path).await?;
                AddTorrent::from_bytes(bytes)
            }
        };
        let options = AddTorrentOptions {
            output_folder: Some(output_dir.to_string_lossy().to_string()),
            only_files: if config.selected_files.is_empty() {
                None
            } else {
                Some(
                    config
                        .selected_files
                        .iter()
                        .map(|value| *value as usize)
                        .collect(),
                )
            },
            overwrite: true,
            peer_limit: config.max_connections.map(|value| value as usize),
            trackers: if config.trackers.is_empty() {
                None
            } else {
                Some(config.trackers.iter().map(ToString::to_string).collect())
            },
            ratelimits: librqbit::limits::LimitsConfig {
                download_bps: nonzero_u32(limits.download_bytes_per_second),
                upload_bps: nonzero_u32(limits.upload_bytes_per_second),
            },
            ..Default::default()
        };
        let handle = session
            .add_torrent(add, Some(options))
            .await
            .map_err(bt_error)?
            .into_handle()
            .ok_or_else(|| {
                FluxionError::new(FluxionErrorKind::Unknown, "torrent was not started")
            })?;
        let selected_files = if config.selected_files.is_empty() {
            None
        } else {
            Some(
                config
                    .selected_files
                    .iter()
                    .map(|value| *value as usize)
                    .collect(),
            )
        };
        Ok(RqbitDownload {
            session,
            handle,
            selected_files,
        })
    }

    /// Resolve a magnet link into torrent metadata without starting a download.
    /// Uses librqbit's `list_only` mode, which contacts peers/DHT to fetch the
    /// info dictionary but allocates no storage and downloads no pieces. The
    /// shared process-wide session is reused; the torrent is never added to it.
    ///
    /// This is a network-bound probe: on a dead swarm it can take the full
    /// timeout and still fail. Callers must surface failures gracefully.
    pub async fn resolve_magnet_preview(&self, magnet: &str) -> Result<MagnetPreview> {
        if !magnet.starts_with("magnet:?") {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "invalid magnet link",
            ));
        }

        // librqbit still wants an output folder even in list-only mode. Point
        // it at a temp dir (nothing is written there) and reuse the shared
        // process-wide session.
        let tmp = tempfile::tempdir().map_err(|e| {
            FluxionError::new(
                FluxionErrorKind::Unknown,
                format!("could not create temp dir for magnet resolve: {e}"),
            )
        })?;
        let session = shared_session().await?.clone();

        let add = AddTorrent::from_url(magnet.to_string());
        let options = AddTorrentOptions {
            list_only: true,
            output_folder: Some(tmp.path().to_string_lossy().to_string()),
            ..Default::default()
        };

        // Magnet resolution is swarm-dependent and unbounded; cap it so a dead
        // swarm does not hang the UI forever.
        let resolved = tokio::time::timeout(
            Duration::from_secs(30),
            session.add_torrent(add, Some(options)),
        )
        .await
        .map_err(|_| {
            FluxionError::new(
                FluxionErrorKind::Network,
                "could not resolve magnet (timed out after 30s) — the swarm may be dead",
            )
        })?
        .map_err(bt_error)?;

        let list = match resolved {
            AddTorrentResponse::ListOnly(resp) => resp,
            // AlreadyManaged/Added should not happen with list_only: true, but
            // handle defensively rather than panicking.
            _ => {
                return Err(FluxionError::new(
                    FluxionErrorKind::Unknown,
                    "magnet resolve did not return a file list",
                ));
            }
        };

        let info_hash = list.info_hash.as_string();
        let name = list
            .info
            .name()
            .map(|cow| cow.into_owned())
            // Fall back to the magnet's `dn` parameter if the info dict has no
            // name (rare, but possible for malformed torrents).
            .or_else(|| librqbit::Magnet::parse(magnet).ok().and_then(|m| m.name));

        let mut total_bytes = 0u64;
        let files = list
            .info
            .iter_file_details()
            .enumerate()
            .map(|(idx, file)| {
                let size = file.len;
                total_bytes += size;
                MagnetPreviewFile {
                    index: idx as u32,
                    name: file.filename.to_string(),
                    size,
                    selected: true,
                }
            })
            .collect::<Vec<_>>();

        let trackers = match librqbit::Magnet::parse(magnet) {
            Ok(m) => m.trackers,
            Err(_) => Vec::new(),
        };

        Ok(MagnetPreview {
            name,
            info_hash,
            total_bytes,
            files,
            trackers,
        })
    }
}

fn nonzero_u32(value: Option<u64>) -> Option<NonZeroU32> {
    let value = value?;
    NonZeroU32::new(value.min(u64::from(u32::MAX)) as u32)
}

fn bt_error(error: anyhow::Error) -> FluxionError {
    FluxionError::new(FluxionErrorKind::Network, error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerIdentity {
    pub client_name: Option<String>,
    pub peer_id: Option<String>,
    pub ip: IpAddr,
}

pub fn should_block_peer(
    peer: &PeerIdentity,
    anti_leech: &AntiLeechConfig,
    ip_filter: &IpFilterConfig,
) -> bool {
    if matches!(
        apply_ip_filter(ip_filter, peer.ip),
        fluxion_core::IpFilterDecision::Deny
    ) {
        return true;
    }
    if let Some(client_name) = &peer.client_name
        && anti_leech
            .blocked_client_names
            .iter()
            .any(|blocked| client_name.contains(blocked))
    {
        return true;
    }
    if let Some(peer_id) = &peer.peer_id
        && anti_leech
            .blocked_peer_id_prefixes
            .iter()
            .any(|prefix| peer_id.starts_with(prefix))
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluxion_core::IpRule;

    #[test]
    fn effective_config_restores_isolated_magnet_and_trackers() {
        let stored = BtTaskConfig {
            source: BtSource::Magnet("magnet:?xt=urn:btih:redacted".to_string()),
            selected_files: Vec::new(),
            trackers: Vec::new(),
            max_connections: None,
            share_ratio_limit: None,
            enable_seeding: false,
            anti_leech: AntiLeechConfig::default(),
            ip_filter: IpFilterConfig::default(),
        };
        let mut credentials = fluxion_core::TaskCredentials::default();
        credentials.extra.insert(
            SECRET_BT_MAGNET.to_string(),
            "magnet:?xt=urn:btih:abc".to_string(),
        );
        credentials.extra.insert(
            format!("{SECRET_BT_TRACKER_PREFIX}000000"),
            "https://tracker.example/announce".to_string(),
        );

        let effective = effective_bt_config(&stored, &credentials).unwrap();
        assert!(matches!(effective.source, BtSource::Magnet(value) if value.ends_with("abc")));
        assert_eq!(effective.trackers.len(), 1);
    }

    #[test]
    fn deny_cidr_blocks_peer() {
        let peer = PeerIdentity {
            client_name: None,
            peer_id: None,
            ip: "10.1.2.3".parse().unwrap(),
        };
        let filter = IpFilterConfig {
            allow: vec![],
            deny: vec![IpRule {
                cidr: "10.0.0.0/8".to_string(),
            }],
        };
        assert!(should_block_peer(
            &peer,
            &AntiLeechConfig::default(),
            &filter
        ));
    }

    #[test]
    fn allow_precedence_over_deny() {
        let peer = PeerIdentity {
            client_name: None,
            peer_id: None,
            ip: "10.1.2.3".parse().unwrap(),
        };
        let filter = IpFilterConfig {
            allow: vec![IpRule {
                cidr: "10.1.2.3".to_string(),
            }],
            deny: vec![IpRule {
                cidr: "10.0.0.0/8".to_string(),
            }],
        };
        assert!(!should_block_peer(
            &peer,
            &AntiLeechConfig::default(),
            &filter
        ));
    }
}
