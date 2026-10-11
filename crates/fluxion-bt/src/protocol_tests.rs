//! Controlled loopback peers; no trackers, DHT, multicast or public content.
use super::*;
use librqbit::{
    CreateTorrentOptions, ListenerOptions, SessionOptions, spawn_utils::BlockingSpawner,
};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn config() -> BtTaskConfig {
    BtTaskConfig {
        source: BtSource::Magnet(String::new()),
        selected_files: vec![],
        trackers: vec![],
        max_connections: Some(4),
        share_ratio_limit: None,
        enable_seeding: false,
        anti_leech: Default::default(),
        ip_filter: Default::default(),
    }
}

async fn session(path: &std::path::Path, name: &str) -> Arc<Session> {
    Session::new_with_opts(
        path.into(),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions {
                listen_addr: "127.0.0.1:0".parse().unwrap(),
                ipv4_only: true,
                ..Default::default()
            }),
            client_name_and_version: Some(name.into()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

struct Fixture {
    _dir: tempfile::TempDir,
    seed: Arc<Session>,
    bytes: Vec<u8>,
    torrent: librqbit::CreateTorrentResult,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = (0..512 * 1024)
            .map(|i| ((i * 31 + i / 251) % 256) as u8)
            .collect::<Vec<_>>();
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        bytes[..8].copy_from_slice(&NEXT.fetch_add(1, Ordering::SeqCst).to_be_bytes());
        let path = dir.path().join("fixture.bin");
        tokio::fs::write(&path, &bytes).await.unwrap();
        let torrent = librqbit::create_torrent(
            &path,
            CreateTorrentOptions {
                piece_length: Some(16384),
                ..Default::default()
            },
            &BlockingSpawner::new(2),
        )
        .await
        .unwrap();
        let seed = session(dir.path(), "FixtureSeed 1.0").await;
        Self {
            _dir: dir,
            seed,
            bytes,
            torrent,
        }
    }
    async fn add_seed(&self, rules: &BtTaskConfig) {
        let h = self
            .seed
            .add_torrent(
                AddTorrent::from_bytes(self.torrent.as_bytes().unwrap()),
                Some(torrent_options(rules, self._dir.path(), Default::default())),
            )
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), h.wait_until_completed())
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_and_magnet_transfer_with_allow_override_produce_exact_bytes() {
    let fixture = Fixture::new().await;
    fixture.add_seed(&config()).await;
    for magnet in [false, true] {
        let output = tempfile::tempdir().unwrap();
        let download = session(output.path(), "FixtureDownload").await;
        let mut rules = config();
        rules.ip_filter.allow.push(fluxion_core::IpRule {
            cidr: "127.0.0.1/32".into(),
        });
        rules.ip_filter.deny.push(fluxion_core::IpRule {
            cidr: "0.0.0.0/0".into(),
        });
        let mut options = torrent_options(&rules, output.path(), Default::default());
        options.initial_peers = Some(vec![fixture.seed.listen_addr().unwrap()]);
        let add = if magnet {
            AddTorrent::from_url(fixture.torrent.as_magnet().to_string())
        } else {
            AddTorrent::from_bytes(fixture.torrent.as_bytes().unwrap())
        };
        let h = download
            .add_torrent(add, Some(options))
            .await
            .unwrap()
            .into_handle()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), h.wait_until_completed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            tokio::fs::read(output.path().join("fixture.bin"))
                .await
                .unwrap(),
            fixture.bytes
        );
        assert!(h.stats().finished);
        download.stop().await;
    }
    fixture.seed.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denied_outgoing_ip_is_never_dialed_including_magnet_metadata() {
    let fixture = Fixture::new().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    for magnet in [false, true] {
        let output = tempfile::tempdir().unwrap();
        let download = session(output.path(), "FixtureDownload").await;
        let mut rules = config();
        // A nonempty allow list must not allow an unlisted address.
        rules.ip_filter.allow.push(fluxion_core::IpRule {
            cidr: "192.0.2.0/24".into(),
        });
        let mut options = torrent_options(&rules, output.path(), Default::default());
        options.initial_peers = Some(vec![listener.local_addr().unwrap()]);
        let add = if magnet {
            AddTorrent::from_url(fixture.torrent.as_magnet().to_string())
        } else {
            AddTorrent::from_bytes(fixture.torrent.as_bytes().unwrap())
        };
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            download.add_torrent(add, Some(options)),
        )
        .await
        .unwrap();
        if magnet {
            assert!(result.is_err(), "denied magnet metadata should not resolve");
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(300), listener.accept())
                .await
                .is_err(),
            "denied peer was dialed (magnet={magnet})"
        );
        download.stop().await;
    }
    fixture.seed.stop().await;
}

fn handshake(hash: &[u8; 20], id: &[u8; 20]) -> Vec<u8> {
    let mut bytes = vec![19];
    bytes.extend_from_slice(b"BitTorrent protocol");
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0x10, 0, 0]);
    bytes.extend_from_slice(hash);
    bytes.extend_from_slice(id);
    bytes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denied_incoming_ip_and_peer_prefix_receive_no_handshake_response() {
    for ip_rule in [true, false] {
        let fixture = Fixture::new().await;
        let mut rules = config();
        if ip_rule {
            rules.ip_filter.deny.push(fluxion_core::IpRule {
                cidr: "127.0.0.0/8".into(),
            });
        } else {
            rules
                .anti_leech
                .blocked_peer_id_prefixes
                .push("-BL0001-".into());
        }
        fixture.add_seed(&rules).await;
        let mut peer = TcpStream::connect(fixture.seed.listen_addr().unwrap())
            .await
            .unwrap();
        peer.write_all(&handshake(
            &fixture.torrent.info_hash().0,
            b"-BL0001-abcdefghijkl",
        ))
        .await
        .unwrap();
        let mut response = [0u8; 68];
        let result = tokio::time::timeout(Duration::from_secs(2), peer.read(&mut response))
            .await
            .unwrap();
        assert!(
            matches!(result, Ok(0)) || result.is_err(),
            "denied incoming peer received a handshake"
        );
        fixture.seed.stop().await;
    }
}

struct Probe {
    policy: Arc<dyn librqbit::PeerFilter>,
    blocked_extended: Arc<AtomicBool>,
}
impl librqbit::PeerFilter for Probe {
    fn allow(
        &self,
        addr: std::net::SocketAddr,
        id: Option<&[u8; 20]>,
        client: Option<&[u8]>,
    ) -> bool {
        let allowed = self.policy.allow(addr, id, client);
        if client.is_some() && !allowed {
            self.blocked_extended.store(true, Ordering::SeqCst);
        }
        allowed
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extended_client_rule_disconnects_actual_outgoing_peer() {
    let fixture = Fixture::new().await;
    fixture.add_seed(&config()).await;
    let output = tempfile::tempdir().unwrap();
    let download = session(output.path(), "FixtureDownload").await;
    let mut rules = config();
    rules
        .anti_leech
        .blocked_client_names
        .push("fixtureSEED".into());
    let blocked = Arc::new(AtomicBool::new(false));
    let mut options = torrent_options(&rules, output.path(), Default::default());
    options.peer_filter = Some(Arc::new(Probe {
        policy: options.peer_filter.take().unwrap(),
        blocked_extended: blocked.clone(),
    }));
    options.initial_peers = Some(vec![fixture.seed.listen_addr().unwrap()]);
    let h = download
        .add_torrent(
            AddTorrent::from_bytes(fixture.torrent.as_bytes().unwrap()),
            Some(options),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !blocked.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("extended handshake did not reach policy");
    assert_eq!(h.stats().progress_bytes, 0);
    download.stop().await;
    fixture.seed.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extended_client_rule_disconnects_actual_incoming_peer() {
    let fixture = Fixture::new().await;
    let mut rules = config();
    rules
        .anti_leech
        .blocked_client_names
        .push("blockedClient".into());
    let blocked = Arc::new(AtomicBool::new(false));
    let mut options = torrent_options(&rules, fixture._dir.path(), Default::default());
    options.peer_filter = Some(Arc::new(Probe {
        policy: options.peer_filter.take().unwrap(),
        blocked_extended: blocked.clone(),
    }));
    let h = fixture
        .seed
        .add_torrent(
            AddTorrent::from_bytes(fixture.torrent.as_bytes().unwrap()),
            Some(options),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), h.wait_until_completed())
        .await
        .unwrap()
        .unwrap();
    let mut peer = TcpStream::connect(fixture.seed.listen_addr().unwrap())
        .await
        .unwrap();
    peer.write_all(&handshake(
        &fixture.torrent.info_hash().0,
        b"-ZZ0001-abcdefghijkl",
    ))
    .await
    .unwrap();
    let mut reply = [0u8; 68];
    tokio::time::timeout(Duration::from_secs(2), peer.read_exact(&mut reply))
        .await
        .unwrap()
        .unwrap();
    // BEP 10 extended handshake: message ID 20, extension ID 0, then bencode.
    let mut ext = vec![20, 0];
    ext.extend_from_slice(b"d1:md11:ut_metadatai1ee1:v13:BlockedCliente");
    let mut packet = (ext.len() as u32).to_be_bytes().to_vec();
    packet.extend(ext);
    peer.write_all(&packet).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !blocked.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("incoming extended handshake did not reach policy");
    let mut discarded = Vec::new();
    let result =
        tokio::time::timeout(Duration::from_secs(2), peer.read_to_end(&mut discarded)).await;
    assert!(result.is_ok(), "blocked peer connection remained open");
    assert_eq!(h.stats().uploaded_bytes, 0);
    fixture.seed.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_resume_verifies_and_repairs_corrupt_pieces() {
    let fixture = Fixture::new().await;
    fixture.add_seed(&config()).await;
    let output = tempfile::tempdir().unwrap();
    let mut existing = fixture.bytes.clone();
    existing[20000] ^= 0xff;
    tokio::fs::write(output.path().join("fixture.bin"), existing)
        .await
        .unwrap();
    let download = session(output.path(), "FixtureDownload").await;
    let mut options = torrent_options(&config(), output.path(), Default::default());
    options.initial_peers = Some(vec![fixture.seed.listen_addr().unwrap()]);
    let h = download
        .add_torrent(
            AddTorrent::from_bytes(fixture.torrent.as_bytes().unwrap()),
            Some(options),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), h.wait_until_completed())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tokio::fs::read(output.path().join("fixture.bin"))
            .await
            .unwrap(),
        fixture.bytes
    );
    assert!(h.stats().finished);
    download.stop().await;
    fixture.seed.stop().await;
}

#[test]
fn global_rules_invalid_rules_and_mapped_ipv6_are_enforced() {
    let settings = fluxion_core::SettingsSnapshot {
        bt_ip_deny: vec!["127.0.0.0/8".into()],
        bt_blocked_clients: vec!["transmission".into()],
        bt_blocked_peer_ids: vec!["-BL".into()],
        bt_trackers: vec!["http://127.0.0.1:1/announce".parse().unwrap()],
        ..Default::default()
    };
    let cfg = runtime_config(&config(), &Default::default(), &settings).unwrap();
    let policy = PeerPolicy::from_config(&cfg);
    assert!(!policy.allow("[::ffff:127.0.0.1]:5000".parse().unwrap(), None, None));
    assert!(!policy.allow(
        "192.0.2.1:5000".parse().unwrap(),
        Some(b"-BL0001-abcdefghijkl"),
        None
    ));
    assert!(!policy.allow(
        "192.0.2.1:5000".parse().unwrap(),
        Some(b"-TR4000-abcdefghijkl"),
        None
    ));
    assert_eq!(
        torrent_options(&cfg, std::path::Path::new("/tmp"), Default::default())
            .trackers
            .unwrap()
            .len(),
        1
    );
    let invalid = fluxion_core::SettingsSnapshot {
        bt_ip_deny: vec!["127.0.0.0/99".into()],
        ..Default::default()
    };
    assert!(runtime_config(&config(), &Default::default(), &invalid).is_err());
    let mut cfg = config();
    cfg.anti_leech
        .blocked_client_names
        .push("transmission".into());
    assert!(!PeerPolicy::from_config(&cfg).allow(
        "127.0.0.1:1".parse().unwrap(),
        Some(b"-TR4000-abcdefghijkl"),
        None
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_stalled_magnet_metadata_stops_waiting_without_managing_a_torrent() {
    let fixture = Fixture::new().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let peer = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut handshake = [0u8; 68];
        stream.read_exact(&mut handshake).await.unwrap();
        seen_tx.send(()).unwrap();
        // Keep the socket open without replying to its actual BT handshake.
        std::future::pending::<()>().await;
    });
    let output = tempfile::tempdir().unwrap();
    let download = session(output.path(), "StalledMetadata").await;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let control = TaskControl::new(
        cancellation.clone(),
        Default::default(),
        Arc::new(Default::default()),
    );
    let mut options = torrent_options(&config(), output.path(), Default::default());
    options.initial_peers = Some(vec![peer]);
    let magnet = fixture.torrent.as_magnet().to_string();
    let task_session = download.clone();
    let task = tokio::spawn(async move {
        controlled_add(
            &task_session,
            AddTorrent::from_url(magnet),
            options,
            &control,
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(5), seen_rx)
        .await
        .unwrap()
        .unwrap();
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(ref e) if e.kind() == FluxionErrorKind::Cancelled));
    assert_eq!(download.with_torrents(|torrents| torrents.count()), 0);
    server.abort();
    let _ = server.await;
    download.stop().await;
    fixture.seed.stop().await;
}
