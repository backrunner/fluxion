---
name: fluxion-requirements-guard
description: Requirement calibration guard for Fluxion. Use when defining, changing, reviewing, or implementing Fluxion features, especially task lifecycle, protocol support, HTTP/BT/FTP/SFTP behavior, Chrome extension handoff, limits, proxy, persistence, security-sensitive headers, or App/Core scope decisions.
---

# Fluxion Requirements Guard

## Required Inputs

Before changing product behavior, read:

- `.agents/requirements.md`
- `.agents/development-plan.md` when sequencing or milestone scope matters
- `.agents/module-design.md` when the change crosses App/Core boundaries

## Calibration Workflow

1. Classify the requested change as M0-M9 if possible.
2. State whether it belongs to current HTTP-first scope or later BT/FTP/SFTP/Chrome scope.
3. Identify affected surfaces: Core API, engine, storage, platform, Tauri bridge, UI, tests.
4. Preserve the current priority: macOS first, HTTP/HTTPS segmented download first, BT second, FTP/SFTP later.
5. Convert ambiguous behavior into explicit acceptance criteria before implementation.

## Hard Requirements

- Do not let App code depend on protocol implementation internals.
- Do not implement BT, FTP, SFTP, or Chrome extension features in a way that blocks the HTTP milestone unless explicitly requested.
- Do not store Cookie, Authorization, or token-like values as ordinary plaintext task metadata.
- Keep task lifecycle semantics consistent: create, start, pause, stop, delete, resume, fail, complete.
- Keep global limits and task limits both enforceable; task-level settings must not bypass global settings.
- Treat unsupported HTTP Range as a normal fallback path, not an error.
- Treat resumability as conditional on stable resource identity, using ETag, Last-Modified, length, or future stronger validators.

## Acceptance Criteria Pattern

For each feature, define:

- User-visible behavior.
- Core behavior.
- Persistence behavior.
- Error behavior.
- Security/privacy behavior.
- Test coverage.

## Stop And Clarify

Ask the user before proceeding only when:

- The request conflicts with `.agents/requirements.md`.
- The request changes priority, such as implementing BT before HTTP is usable.
- The request weakens credential storage, logging, or path safety.
- A product decision affects data migration or irreversible user data behavior.

