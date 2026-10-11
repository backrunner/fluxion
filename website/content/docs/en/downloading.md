---
title: "Your first download"
description: "Add a link, choose a destination and follow progress."
order: 1
---

## Add a task

1. Select **New** in the download workspace.
2. Paste an HTTP, HTTPS, FTP or SFTP URL, a magnet link, or choose a local `.torrent` file.
3. Choose a download folder. For a sandbox build, use the folder picker to grant access outside Downloads.
4. Review the filename and available advanced settings, then create the task.

For magnet links, resolve the file list before choosing individual files. Resolution needs reachable peers or trackers and can take time. Filenames and paths supplied by remote sources are checked before writing.

## Advanced settings

HTTP transfers can use custom headers, cookies, Referer and User-Agent. Credentials and signed links can grant access to private content: use only sources you trust and never include these values in public reports.

FTP and SFTP can use a username and password. SFTP can also use a private key selected through the file picker and an optional passphrase. BitTorrent offers file selection, extra trackers, seeding and a share-ratio limit.

## Resume and limits

HTTP resume depends on Range support and source validators; a changed resource can require restarting. FTP resume requires server support for REST. SFTP can resume a local partial file against the remote source.

Set download/upload limits for a task or in Settings. Rates in the settings form use **KB/s**; an empty value means no configured limit. Some server or protocol conditions can still limit actual speed.

## Find the file

When the task completes, use its folder action to reveal the downloaded file in Finder. A completed task records a finished transfer; moving or deleting a file outside Fluxion can make its saved location unavailable.
