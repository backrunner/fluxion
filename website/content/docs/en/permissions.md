---
title: "Files and permissions"
description: "Choose destinations and understand saved access."
order: 4
---

## Choose the download folder

Fluxion writes to the destination you choose. In the sandboxed build, Downloads is supported through its entitlement, while other folders should be selected with the system picker. A security-scoped bookmark saves that user-selected access for later launches.

A bookmark is a local access grant, not a cloud permission or a copy of the folder. Moving the folder, disconnecting a drive, revoking access or changing permissions can make it unavailable. If access fails, select the folder again and inspect the task before resuming.

## Torrent and private-key files

Use the file picker for local torrent files and SFTP private-key files, particularly in the sandboxed edition. A path typed into a field does not itself grant sandbox access. Keep private keys in a protected location and never send them to support.

## Finder actions

Use the completed task’s folder action to reveal its files in Finder. If you moved or deleted a file outside Fluxion, the stored path may no longer identify it. Fluxion checks task file paths before app-initiated operations.

## Removing files

Removing a task and removing its downloaded files are distinct actions. Review the confirmation carefully. Downloaded files may remain after uninstalling the app; manage those files in Finder according to your needs.

## Cloud and removable storage

External drives must be mounted. Cloud-backed folders may have their own availability and synchronization behavior. Confirm enough free space and keep the destination available while transferring.
