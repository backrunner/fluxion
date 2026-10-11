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
            Session::new_with_opts(
                base,
                librqbit::SessionOptions {
                    listen: Some(librqbit::ListenerOptions::default()),
                    // Core owns task recovery. Avoid librqbit's independent
                    // home-directory DHT cache, which is outside store authority.
                    dht: Some(librqbit::DhtSessionConfig {
                        persistence: None,
                        ..Default::default()
                    }),
                    persistence: None,
                    ..Default::default()
                },
            )
            .await
            .map_err(bt_error)
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

    async fn prepare(&self, ctx: EngineContext, task: PreparedTask) -> Result<PreparedTask> {
        let TaskKind::Bt(stored_config) = &task.task.kind else {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "not a BT task",
            ));
        };
        let config = runtime_config(
            stored_config,
            &task.credentials,
            &ctx.storage.get_settings().await?,
        )?;
        validate_peer_policy(&config)?;
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
        let config = runtime_config(
            stored_config,
            &task.credentials,
            &ctx.storage.get_settings().await?,
        )?;
        let handle = self
            .adapter
            .start(
                &config,
                task.task.save_dir.clone(),
                task.task.limits.clone(),
                &control,
            )
            .await?;
        let result = self
            .run_handle(&ctx, &task, &config, control, &handle)
            .await;
        // Always stop the managed torrent, including progress/storage failures.
        // Otherwise a Failed task can keep receiving or uploading in the session.
        handle.remove().await?;
        result
    }

    async fn resolve_magnet_preview(
        &self,
        ctx: EngineContext,
        magnet: &str,
    ) -> Result<MagnetPreview> {
        self.adapter
            .resolve_magnet_preview(magnet, &ctx.storage.get_settings().await?)
            .await
    }
}

impl BtEngine {
    async fn run_handle(
        &self,
        ctx: &EngineContext,
        task: &PreparedTask,
        config: &BtTaskConfig,
        control: TaskControl,
        handle: &RqbitDownload,
    ) -> Result<EngineExit> {
        sync_session_rate_limits(ctx, handle).await?;
        // Register the live handle as a state provider so the UI can query
        // file/piece state while the task is running (or seeding).
        let provider: Arc<dyn EngineStateProvider> = Arc::new(handle.clone());
        ctx.register_state_provider(task.task.id, provider).await;
        loop {
            if let Err(error) = sync_session_rate_limits(ctx, handle).await {
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
                return Err(FluxionError::cancelled());
            }
            let cancellation = control.cancellation_token();
            tokio::select! {
                _ = cancellation.cancelled() => {},
                _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            }
        }
        if config.enable_seeding {
            ctx.set_state(task.task.id, TaskState::Seeding).await?;
            // The download is already complete at this point, so both a
            // reached share ratio and a user cancellation of the seeding
            // phase end the task as Completed rather than as cancelled.
            self.seed_until_cancelled_or_ratio(ctx, task, config, control, handle)
                .await?;
        }
        Ok(EngineExit::Completed { file_path: None })
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
            let cancellation = control.cancellation_token();
            tokio::select! {
                _ = cancellation.cancelled() => {},
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
            }
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

async fn controlled_add(
    session: &Arc<Session>,
    add: AddTorrent<'_>,
    options: AddTorrentOptions,
    control: &TaskControl,
) -> Result<AddTorrentResponse> {
    control
        .network_io(async {
            session
                .add_torrent(add, Some(options))
                .await
                .map_err(bt_error)
        })
        .await
}

impl RqbitAdapter {
    pub async fn start(
        &self,
        config: &BtTaskConfig,
        output_dir: PathBuf,
        limits: TaskRateLimit,
        control: &TaskControl,
    ) -> Result<RqbitDownload> {
        validate_peer_policy(config)?;
        tokio::fs::create_dir_all(&output_dir).await?;
        let session = shared_session().await?.clone();
        let add = match &config.source {
            BtSource::Magnet(value) => AddTorrent::from_url(value.clone()),
            BtSource::TorrentFile(path) => {
                let bytes = tokio::fs::read(path).await?;
                AddTorrent::from_bytes(bytes)
            }
        };
        let options = torrent_options(config, &output_dir, limits);
        // Metadata resolution can otherwise wait indefinitely for a dead swarm.
        // Our shared session has no persistence backend: after insertion there is
        // no await before returning the handle, so dropping this wait cannot
        // strand a registered torrent between insertion and handle ownership.
        let handle = match controlled_add(&session, add, options, control).await? {
            AddTorrentResponse::Added(_, handle) => handle,
            AddTorrentResponse::AlreadyManaged(_, _) => {
                return Err(FluxionError::new(
                    FluxionErrorKind::InvalidConfig,
                    "A task for this torrent is already active",
                ));
            }
            AddTorrentResponse::ListOnly(_) => {
                return Err(FluxionError::new(
                    FluxionErrorKind::Unknown,
                    "torrent was not started",
                ));
            }
        };
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
    pub async fn resolve_magnet_preview(
        &self,
        magnet: &str,
        settings: &fluxion_core::SettingsSnapshot,
    ) -> Result<MagnetPreview> {
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
            trackers: Some(
                settings
                    .bt_trackers
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
            peer_filter: Some(PeerPolicy::from_config(&runtime_config(
                &BtTaskConfig {
                    source: BtSource::Magnet(magnet.to_owned()),
                    selected_files: vec![],
                    trackers: vec![],
                    max_connections: None,
                    share_ratio_limit: None,
                    enable_seeding: false,
                    anti_leech: Default::default(),
                    ip_filter: Default::default(),
                },
                &Default::default(),
                settings,
            )?)),
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

// Merge persisted credentials and global policy before handing the per-torrent
// filter to librqbit. Reject behavior rules the adapter cannot enforce.
fn runtime_config(
    stored: &BtTaskConfig,
    credentials: &fluxion_core::TaskCredentials,
    settings: &fluxion_core::SettingsSnapshot,
) -> Result<BtTaskConfig> {
    let mut config = effective_bt_config(stored, credentials)?;
    config.trackers.extend(settings.bt_trackers.iter().cloned());
    config.trackers.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    config.trackers.dedup();
    config.ip_filter.allow.extend(
        settings
            .bt_ip_allow
            .iter()
            .map(|cidr| fluxion_core::IpRule { cidr: cidr.clone() }),
    );
    config.ip_filter.deny.extend(
        settings
            .bt_ip_deny
            .iter()
            .map(|cidr| fluxion_core::IpRule { cidr: cidr.clone() }),
    );
    config
        .anti_leech
        .blocked_client_names
        .extend(settings.bt_blocked_clients.iter().cloned());
    config
        .anti_leech
        .blocked_peer_id_prefixes
        .extend(settings.bt_blocked_peer_ids.iter().cloned());
    validate_peer_policy(&config)?;
    Ok(config)
}

fn torrent_options(
    config: &BtTaskConfig,
    output_dir: &std::path::Path,
    limits: TaskRateLimit,
) -> AddTorrentOptions {
    AddTorrentOptions {
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
        peer_filter: Some(PeerPolicy::from_config(config)),
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
    }
}

fn validate_peer_policy(config: &BtTaskConfig) -> Result<()> {
    if config.anti_leech.block_suspicious_fast_disconnects {
        return Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "Automatic fast-disconnect blocking is not supported. Disable this behavior rule before starting.",
        ));
    }
    for rule in config.ip_filter.allow.iter().chain(&config.ip_filter.deny) {
        if fluxion_core::cidr_contains(&rule.cidr, "127.0.0.1".parse().unwrap()).is_none()
            && fluxion_core::cidr_contains(&rule.cidr, "::1".parse().unwrap()).is_none()
        {
            return Err(FluxionError::new(
                FluxionErrorKind::InvalidConfig,
                "Invalid peer IP or CIDR rule",
            ));
        }
    }
    if config
        .anti_leech
        .blocked_client_names
        .iter()
        .chain(&config.anti_leech.blocked_peer_id_prefixes)
        .any(|rule| rule.trim().is_empty())
    {
        return Err(FluxionError::new(
            FluxionErrorKind::InvalidConfig,
            "Peer blocking rules cannot be empty",
        ));
    }
    Ok(())
}

struct PeerPolicy {
    anti_leech: AntiLeechConfig,
    ip_filter: IpFilterConfig,
}
impl PeerPolicy {
    fn from_config(config: &BtTaskConfig) -> Arc<dyn librqbit::PeerFilter> {
        Arc::new(Self {
            anti_leech: config.anti_leech.clone(),
            ip_filter: config.ip_filter.clone(),
        })
    }
}
impl librqbit::PeerFilter for PeerPolicy {
    fn allow(
        &self,
        addr: std::net::SocketAddr,
        id: Option<&[u8; 20]>,
        client: Option<&[u8]>,
    ) -> bool {
        let decoded_name = id
            .and_then(|id| librqbit::try_decode_peer_id(librqbit::dht::Id20::new(*id)))
            .and_then(|peer| match peer {
                librqbit::PeerId::AzureusStyle(style) => match style.kind {
                    librqbit::AzureusStyleKind::Deluge => Some("Deluge"),
                    librqbit::AzureusStyleKind::LibTorrent => Some("libtorrent"),
                    librqbit::AzureusStyleKind::Transmission => Some("Transmission"),
                    librqbit::AzureusStyleKind::QBittorrent => Some("qBittorrent"),
                    librqbit::AzureusStyleKind::UTorrent => Some("uTorrent"),
                    librqbit::AzureusStyleKind::RQBit => Some("rqbit"),
                    librqbit::AzureusStyleKind::Other(_) => None,
                },
            });
        let mut peer = PeerIdentity {
            ip: addr.ip().to_canonical(),
            peer_id: id.map(|v| String::from_utf8_lossy(v).into_owned()),
            client_name: decoded_name.map(str::to_owned),
        };
        if should_block_peer(&peer, &self.anti_leech, &self.ip_filter) {
            return false;
        }
        peer.client_name = client.map(|v| String::from_utf8_lossy(v).into_owned());
        !should_block_peer(&peer, &self.anti_leech, &self.ip_filter)
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
        && anti_leech.blocked_client_names.iter().any(|blocked| {
            client_name
                .to_lowercase()
                .contains(&blocked.trim().to_lowercase())
        })
    {
        return true;
    }
    if let Some(peer_id) = &peer.peer_id
        && anti_leech
            .blocked_peer_id_prefixes
            .iter()
            .any(|prefix| peer_id.starts_with(prefix.trim()))
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

        assert!(validate_peer_policy(&stored).is_ok());
        let mut restricted = stored.clone();
        restricted.ip_filter.deny.push(IpRule {
            cidr: "10.0.0.0/8".into(),
        });
        assert!(validate_peer_policy(&restricted).is_ok());
        restricted.ip_filter.deny.clear();
        restricted
            .anti_leech
            .blocked_client_names
            .push("blocked-client".into());
        assert!(validate_peer_policy(&restricted).is_ok());
        restricted.anti_leech.block_suspicious_fast_disconnects = true;
        assert!(validate_peer_policy(&restricted).is_err());
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

#[cfg(test)]
mod protocol_tests;
