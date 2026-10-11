---
title: "Troubleshooting"
description: "Practical checks for interrupted or failed transfers."
order: 5
---

## An HTTP transfer will not resume

Check whether the source URL has expired, the resource changed, or the server supports byte ranges. Authentication can require fresh cookies or headers. A server that rejects segmented requests may need single-stream transfer. Retry with a valid source rather than editing local partial files blindly.

## FTP or SFTP fails

Confirm the hostname, port, username and authentication method. FTP is not FTPS; FTPS URLs are intentionally unsupported. Check server resume support for FTP. SFTP requires a matching host key after the first connection. A changed-key error should be investigated with the server operator, not ignored.

If a network wait stalls, pause the task. Network operations have time bounds, and partial local data is preserved where possible. A stopped server cannot be made available by repeated retries.

## A magnet has no file list or a torrent has no peers

Metadata needs reachable peers or trackers. Check your tracker list, network access and connection rules. A nonempty allow list rejects all peers outside it. Blocked client names and Peer ID prefixes can also exclude available peers. Rules do not take effect on an already running task until restarted.

Torrent content must match its piece hashes. Fluxion’s engine checks pieces; damaged existing data may need redownloading. No available seeds can mean there is no source for the missing pieces.

## A download folder is unavailable

Reconnect the drive, check permissions and free space, then reselect the folder through the picker in a sandboxed build. See [files and permissions](/docs/permissions).

## A batch action only partly succeeded

Each task is processed independently. Read the reported failures and inspect those tasks, while checking which other tasks completed their requested action.

## Reporting a problem

Review the [support page](/support). Include reproducible steps and redacted error text, never passwords, signed URLs, private tracker addresses or private keys. The OnFire private support channel is being prepared and is not currently open.
