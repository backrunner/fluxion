---
name: fluxion-bt-extension-guard
description: BitTorrent extension guard for Fluxion. Use when designing, implementing, or reviewing BT support, torrent files, magnet links, tracker list configuration, DHT/peer connections, UPnP, seeding, share ratio limits, piece and file progress, anti-leech behavior, connection limits, or IP filtering.
---

# Fluxion BT Extension Guard

## Required Inputs

Read `.agents/requirements.md` section 4.5 and `.agents/module-design.md` section 5 before BT work. Check `.agents/development-plan.md` milestones M6-M7 for sequencing.

## Scope Rules

- Do not let BT work destabilize the HTTP-first milestone.
- Keep BT inside `fluxion-bt` behind the same engine abstraction.
- Reuse Core task lifecycle, event bus, storage, rate limiters, proxy policy, and settings.
- App reads BT status through Core detail DTOs, not raw BT internals.

## Required BT Capabilities

Support or preserve clear extension points for:

- torrent file tasks.
- magnet link tasks.
- file selection at creation.
- tracker list configuration.
- global custom tracker list.
- piece progress.
- file-level progress and speed.
- max connection limits.
- upload and download limits.
- completed-task seeding.
- share ratio stop threshold.
- UPnP/NAT-PMP.
- anti-leech client blocking.
- IP allow/deny lists with CIDR support.

## Design Rules

- Evaluate mature Rust BT crates before self-implementing protocol details.
- Persist enough piece and file state for crash recovery.
- Verify every completed piece against its hash.
- Avoid loading entire torrent payloads or files into memory.
- Use bounded peer connections and queues.
- Make tracker, peer, and piece updates event-throttled for UI.

## IP Filtering Rules

- Support individual IPs and CIDR subnets.
- Define precedence explicitly when both allow and deny rules match.
- Apply filters before accepting or initiating peer connections.
- Keep rule parsing test-covered.

## Completion Checklist

- BT uses Core limits and lifecycle.
- Selected files map correctly to piece requests.
- Piece hash validation is mandatory.
- Seeding and share ratio behavior are persisted.
- App can show torrent-level, file-level, and piece-level status without protocol leakage.

