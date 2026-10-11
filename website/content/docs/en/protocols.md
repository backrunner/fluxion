---
title: "Protocol support"
description: "Transfer capabilities, privacy boundaries and peer rules."
order: 3
---

## Supported protocols

| Protocol | Current behavior |
| --- | --- |
| HTTP / HTTPS | Redirect and Range probing, segmented transfer, resume checks, retries and single-stream fallback |
| BitTorrent | Torrent files and magnets, trackers/DHT, file selection, piece/file state, seeding and rate limits |
| FTP | File transfer and partial-file resume where the server supports it |
| SFTP | Password or private-key authentication, partial-file resume and saved host-key checks |
| FTPS | Not supported; explicitly rejected |

## BitTorrent connection rules

Settings provides IP allow/deny lists, blocked client names and blocked Peer ID prefixes. Enter one rule per line. IP rules can be single addresses or CIDR ranges.

Allow rules take priority over deny rules. **When the allow list is nonempty, peers outside it are rejected.** IPv4-mapped IPv6 addresses are checked against the corresponding IPv4 rules. Client-name matching is case-insensitive; Peer ID prefix matching is case-sensitive.

Rules apply to newly started or restarted tasks and magnet previews. Active tasks keep their current connection policy. Client names and Peer IDs are self-reported or inferred from a peer’s ID and can be spoofed. These rules are not an identity verification or anonymity service.

Rapid-disconnect behavior blocking is not implemented. Do not rely on it as an anti-abuse feature. Router port forwarding and NAT conditions can affect inbound connectivity; no automatic UPnP setup is promised.

## Sharing and rights

BitTorrent can upload pieces during a download. Post-download seeding continues only when enabled and is subject to the configured share limit. Download and share only content you are authorized to access and distribute.

## SFTP host keys

The first connection saves the observed host fingerprint (trust on first use). Later connections reject changed keys. This protects against an unexpected change but does not independently prove the first server’s identity. If the key changes, verify the new fingerprint with the server operator through a trusted channel before proceeding. Fluxion does not silently replace the saved key.

## Proxy and encryption

System-proxy behavior depends on the protocol and platform settings; it is not an anonymity guarantee. FTP sends data and credentials without encryption. Use HTTPS or SFTP for sensitive transfers. Private tracker URLs and magnet parameters may carry credentials: keep them out of public reports.
