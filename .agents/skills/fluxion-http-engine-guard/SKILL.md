---
name: fluxion-http-engine-guard
description: HTTP download engine guard for Fluxion. Use when implementing, modifying, or reviewing HTTP/HTTPS downloads, redirects, HEAD/Range probing, multi-thread segmented downloads, fallback to single-thread downloads, resume, retry, file writes, custom headers, cookies, proxy, or download performance.
---

# Fluxion HTTP Engine Guard

## Required Inputs

Read:

- `.agents/requirements.md` section 4.2
- `.agents/module-design.md` section 4
- `.agents/development-plan.md` milestones M1-M4

## Probe Rules

- Follow redirects with a bounded redirect count.
- Try HEAD first for metadata.
- If HEAD fails or is incomplete, use `GET` with `Range: bytes=0-0`.
- Enable segmented download only when the server proves Range support with `206 Partial Content` and a parseable total length.
- Fall back to single-thread download when Range is unsupported, ignored, unknown, or the file is too small.
- Treat 401/403 as credential problems, 404 as not found, 416 as stale or invalid range metadata.

## Segmentation Rules

- Default max connections: 16.
- Respect user task max connections and any global cap.
- Do not create segments smaller than the configured minimum unless unavoidable.
- Persist each segment: start, end, downloaded, state, retry count, last error.
- Resume each segment from `start + downloaded`.
- Revalidate resource identity before resuming when validators exist.

## IO And Memory Rules

- Stream response bodies; never buffer full files in memory.
- Use fixed-size buffers, typically 64 KiB to 256 KiB.
- Pre-size or preallocate the temp file when length is known.
- Write by absolute offset through a single writer abstraction.
- Download to a temp path and atomically move to the final path after validation.
- Validate final file size; use hash validation when a trusted hash is available.

## Retry And Cancellation Rules

- Retry timeouts, disconnects, 429, 5xx, and transient DNS failures.
- Do not blindly retry 401, 403, 404, permission denied, disk full, or validator mismatch.
- Use bounded retries with backoff.
- Pause and stop must stop network reads promptly and flush segment progress.

## Headers And Proxy Rules

- Support custom headers.
- Treat Cookie, Authorization, Proxy-Authorization, and token-like headers as sensitive.
- Do not log sensitive header values.
- Honor system proxy policy through Core/platform abstractions.

## Required Tests

Use a local HTTP server to cover:

- 302 redirect.
- HEAD unsupported.
- Range supported with 206.
- Range ignored with 200 fallback.
- 416 stale range.
- interrupted connection and retry.
- Cookie/Header-required download.
- final hash or byte-for-byte equality after segmented download.

