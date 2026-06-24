---
name: fluxion-quality-gate
description: Quality gate for Fluxion changes. Use before finishing any implementation or review to choose tests, run verification, check regressions, validate HTTP/BT/App behavior, enforce security redaction, and report remaining risks.
---

# Fluxion Quality Gate

## Required Inputs

Read `.agents/development-plan.md` section 6 for test strategy. Read the relevant guard skill for the area touched.

## Verification Ladder

Choose the narrowest sufficient checks, then broaden for shared behavior:

1. Formatting and static analysis.
2. Unit tests for pure logic.
3. Integration tests for Core engines and storage.
4. Local HTTP server tests for HTTP behavior.
5. Tauri command tests for App/Core bridge.
6. UI preview or browser verification for visible frontend changes.

## Required Checks By Area

Core lifecycle:

- create, start, pause, stop, delete.
- state persistence.
- event emission.
- cancellation behavior.

HTTP:

- redirect.
- HEAD fallback.
- 206 segmented download.
- 200 fallback when Range is ignored.
- resume.
- retry.
- final file size/hash.
- limits.

Storage:

- migrations.
- crash recovery state.
- segment progress persistence.
- SecretRef handling.

Security:

- log redaction.
- unsafe filename/path handling.
- sensitive header storage.

App:

- command DTO serialization.
- event subscription.
- loading/error/empty states.
- invalid action disabled states.

BT, when implemented:

- torrent and magnet parsing.
- file selection.
- piece hash validation.
- tracker config.
- IP filter CIDR behavior.
- seeding/share ratio limits.

## Reporting Rules

Final reports should include:

- what changed.
- verification commands run.
- important results.
- tests not run and why.
- remaining risks when any.

Do not claim a behavior is verified unless a relevant command, test, or manual check was actually performed.

