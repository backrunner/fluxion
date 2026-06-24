---
name: fluxion-transfer-security-guard
description: Security and privacy guard for Fluxion transfers. Use when handling cookies, Authorization headers, custom headers, Chrome extension handoff, proxy credentials, SFTP credentials, Keychain/SecretStore, logs, diagnostics, download paths, file deletion, or any code that stores or displays sensitive transfer data.
---

# Fluxion Transfer Security Guard

## Required Inputs

Read `.agents/requirements.md` sections 4.7, 5.4, and 7. Read `.agents/module-design.md` sections 7, 9, and 11 when storage, proxy, or Chrome integration is involved.

## Sensitive Data

Treat these as secrets:

- `Cookie`
- `Authorization`
- `Proxy-Authorization`
- bearer tokens and API keys
- signed URLs when query params carry credentials
- SFTP passwords and private key passphrases
- proxy credentials

## Storage Rules

- Prefer macOS Keychain through `SecretStore`.
- Store only `SecretRef` in normal task tables.
- Do not write secrets to plaintext JSON, SQLite columns, crash reports, snapshots, or UI state persistence.
- Delete associated secrets when deleting a task unless another task still references them.

## Logging And Diagnostics

- Redact sensitive headers and credential-bearing URL query params.
- Diagnostic export must default to redacted output.
- Never print raw request headers in normal logs.
- Include enough non-secret context for debugging: task id, status code, error kind, retry count, final host, range metadata.

## Path And File Safety

- Normalize and validate save paths.
- Prevent path traversal from Content-Disposition or URLs.
- Confirm destructive delete behavior at the App boundary.
- Use temp files during downloads and atomic moves on completion.
- Never follow untrusted path suggestions outside the selected save directory.

## Chrome Extension Rules

- Use Native Messaging for local handoff.
- Transfer only headers needed for the target download.
- Validate message origin and schema.
- Allow user confirmation or a clearly configured auto-capture policy.
- Do not persist extension-provided secrets outside SecretStore.

## Completion Checklist

- Secrets are isolated from ordinary metadata.
- Logs and errors are redacted.
- UI does not expose raw credentials by default.
- Paths are sanitized.
- Tests or review cases cover redaction and unsafe filename handling.

