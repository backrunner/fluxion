use async_trait::async_trait;
use fluxion_core::{
    AntiLeechConfig, BtFileState, BtSource, BtStateSnapshot, BtTaskConfig, DownloadEngine,
    DownloadKind, EngineContext, EngineExit, EngineStateProvider, FluxionError, FluxionErrorKind,
    IpFilterConfig, MagnetPreview, MagnetPreviewFile, PreparedTask, Result, TaskControl, TaskKind,
    TaskRateLimit, TaskState, apply_ip_filter,
};
use librqbit::{AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, Session, api::TorrentIdOrHash};
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, num::NonZeroU32, path::PathBuf, sync::Arc, time::Duration};

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
        let TaskKind::Bt(config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not a BT task",
            ));
        };
        validate_bt_source(&config.source)?;
        Ok(task)
    }

    async fn run(
        &self,
        ctx: EngineContext,
        task: PreparedTask,
        control: TaskControl,
    ) -> Result<EngineExit> {
        let TaskKind::Bt(config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not a BT task",
            ));
        };
        let handle = self
            .adapter
            .start(config, task.task.save_dir.clone(), task.task.limits.clone())
            .await?;
        // Register the live handle as a state provider so the UI can query
        // file/piece state while the task is running (or seeding).
        let provider: Arc<dyn EngineStateProvider> = Arc::new(handle.clone());
        ctx.register_state_provider(task.task.id, provider).await;
        let mut last_progress = (0_u64, 0_u64);
        loop {
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
            let downloaded_delta = stats.progress_bytes.saturating_sub(last_progress.0);
            let uploaded_delta = stats.uploaded_bytes.saturating_sub(last_progress.1);
            if downloaded_delta > 0 {
                control.acquire_download(downloaded_delta).await;
            }
            if uploaded_delta > 0 {
                control.acquire_upload(uploaded_delta).await;
            }
            last_progress = (stats.progress_bytes, stats.uploaded_bytes);
            if stats.finished {
                break;
            }
            if control.is_cancelled() {
                let _ = handle.pause().await;
                return Err(FluxionError::cancelled());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if config.enable_seeding {
            ctx.set_state(task.task.id, TaskState::Seeding).await?;
            if self
                .seed_until_cancelled_or_ratio(&ctx, &task, config, control, handle)
                .await?
            {
                Ok(EngineExit::Completed { file_path: None })
            } else {
                Ok(EngineExit::Seeding)
            }
        } else {
            let _ = handle.pause().await;
            Ok(EngineExit::Completed { file_path: None })
        }
    }

    async fn resolve_magnet_preview(&self, magnet: &str) -> Result<MagnetPreview> {
        self.adapter.resolve_magnet_preview(magnet).await
    }
}

impl BtEngine {
    async fn seed_until_cancelled_or_ratio(
        &self,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &BtTaskConfig,
        control: TaskControl,
        handle: RqbitDownload,
    ) -> Result<bool> {
        loop {
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
                let _ = handle.pause().await;
                return Ok(true);
            }
            if control.is_cancelled() {
                let _ = handle.pause().await;
                return Err(FluxionError::cancelled());
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
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

pub struct RqbitAdapter;

#[derive(Clone)]
pub struct RqbitDownload {
    session: Arc<Session>,
    handle: Arc<librqbit::ManagedTorrent>,
}

impl RqbitDownload {
    fn stats(&self) -> librqbit::TorrentStats {
        self.handle.stats()
    }

    async fn pause(&self) -> Result<()> {
        self.session.pause(&self.handle).await.map_err(bt_error)
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
                        selected: true,
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
        let session = Session::new(output_dir.clone()).await.map_err(bt_error)?;
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
        Ok(RqbitDownload { session, handle })
    }

    /// Resolve a magnet link into torrent metadata without starting a download.
    /// Uses librqbit's `list_only` mode, which contacts peers/DHT to fetch the
    /// info dictionary but allocates no storage and downloads no pieces. The
    /// session runs in a temporary directory that is cleaned up afterward.
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

        // librqbit's Session requires a writable output directory even in
        // list-only mode. Use a temp dir and remove it once we have the
        // metadata.
        let tmp = tempfile::tempdir().map_err(|e| {
            FluxionError::new(
                FluxionErrorKind::Unknown,
                format!("could not create temp dir for magnet resolve: {e}"),
            )
        })?;
        let session = Session::new(tmp.path().to_path_buf())
            .await
            .map_err(bt_error)?;

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
                ))
            }
        };

        let info_hash = list.info_hash.as_string();
        let name = list
            .info
            .name()
            .map(|cow| cow.into_owned())
            // Fall back to the magnet's `dn` parameter if the info dict has no
            // name (rare, but possible for malformed torrents).
            .or_else(|| {
                librqbit::Magnet::parse(magnet)
                    .ok()
                    .and_then(|m| m.name)
            });

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
