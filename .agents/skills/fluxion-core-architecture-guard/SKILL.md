---
name: fluxion-core-architecture-guard
description: Architecture guard for Fluxion Core. Use when designing or modifying Core APIs, task management, scheduler, DownloadEngine traits, storage abstractions, event bus, rate limiting, proxy/platform services, or crate boundaries between Fluxion App and Fluxion Core.
---

# Fluxion Core Architecture Guard

## Required Inputs

Read `.agents/module-design.md` before changing Core structure. Read `.agents/requirements.md` if behavior or scope changes.

## Boundary Rules

- Keep Core independent of Tauri UI and frontend state.
- Keep protocol engines behind a `DownloadEngine`-style abstraction.
- Keep shared concerns in Core or shared crates: task lifecycle, scheduler, events, storage, limits, proxy policy, settings, errors.
- Keep HTTP-specific code in `fluxion-http`.
- Keep macOS Keychain, system proxy, Finder/open-file behavior in `fluxion-platform` or Tauri bridge, not in protocol engines.
- Keep DTO conversion at the Tauri bridge boundary.

## Core API Rules

Core APIs should expose stable operations:

- `create_task`
- `start_task`
- `pause_task`
- `stop_task`
- `delete_task`
- `list_tasks`
- `get_task`
- `subscribe`
- settings read/update APIs when needed

Do not expose worker handles, raw network clients, protocol-specific mutable internals, or database rows directly to the App.

## Event Rules

- Emit state changes immediately.
- Throttle progress and speed events.
- Make events serializable for Tauri.
- Do not rely on frontend polling as the main update path.
- Persist authoritative state in storage; events are notifications, not durable state.

## Storage Rules

- Store common task data separately from protocol-specific tables.
- Store HTTP segment state explicitly enough for crash recovery.
- Use a SecretStore reference for sensitive credentials.
- Batch or throttle high-frequency progress writes.
- Prefer migrations over destructive schema changes once user data exists.

## Concurrency Rules

- Use cancellation tokens or equivalent structured cancellation for pause/stop.
- Avoid unbounded tasks, queues, channels, and retries.
- Make rate limiters shared and runtime-updatable.
- Ensure task shutdown waits for file metadata flush and segment state persistence.

## Completion Checklist

- App/Core dependency direction is unchanged.
- New protocol behavior fits the engine abstraction.
- New task state is persisted and emitted.
- Errors map to user-readable error kinds.
- Tests cover lifecycle, persistence, and cancellation for the touched path.

