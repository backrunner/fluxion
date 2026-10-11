# Fluxion librqbit patch

Upstream: https://github.com/ikatson/rqbit
Crates.io package: librqbit 9.0.0-rc.0
Original package SHA-256: `b9451def41c18d1b8e7b68c097bcb0c9a0579cdc9b779803368a65b86cae03d2`
License: Apache-2.0; upstream author Igor Katson and rqbit contributors.

The registry source is retained for reproducibility, with its normalized Cargo
manifest. Cache markers, original workspace manifest and independent lockfile
are excluded. `LICENSE` contains the standard Apache-2.0 text.

Fluxion modifications:

- Runtime-only `AddTorrentOptions.peer_filter`, propagated to live connections
  and magnet metadata readers; no new persistent protocol fields.
- Outgoing checks before TCP/uTP dialing; incoming checks after a bounded
  handshake identifies the torrent and before sending a handshake response.
- Peer ID checks before message handling; extended client-name checks before
  the extended handshake reaches torrent/metadata handlers.
- Custom trackers included in the magnet branch, matching torrent-file behavior.

Client names and Peer IDs are self-reported preference inputs, not authenticated
identities. Incoming sockets must be accepted to identify a torrent. IP/ID-denied
peers receive no handshake response or pieces. Tracker servers and DHT nodes
are outside torrent-peer IP rules. An extended-name rule takes effect once that
name is announced; it cannot authenticate a peer that lies about its name.

Loopback protocol regression tests are in
`crates/fluxion-bt/src/protocol_tests.rs`; no public content, trackers or DHT.

When upgrading, review changes in `src/lib.rs`, `src/peer_filter.rs`,
`src/peer_connection.rs`, `src/peer_info_reader/mod.rs`, `src/dht_utils.rs`,
`src/session.rs`, `src/torrent_state/mod.rs`, `src/torrent_state/live/mod.rs`.
