//! Embedding applications may deny peers without changing protocol handling.
use std::net::SocketAddr;

/// Called before dialing and on both handshake stages. Returning false closes
/// the connection before passing the peer to torrent/metadata message handlers.
/// Incoming torrent-specific checks run after reading the info hash, before a
/// handshake response or any torrent data is sent. Peer identities are claims,
/// not authenticated identities. Callbacks must be fast and must not block.
pub trait PeerFilter: Send + Sync {
    fn allow(&self, addr: SocketAddr, peer_id: Option<&[u8; 20]>, client: Option<&[u8]>) -> bool;
}
