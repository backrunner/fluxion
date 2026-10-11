---
title: "Managing tasks"
description: "Pause, retry, filter and restore transfers."
order: 2
---

## Keep the workspace clear

Use search to find tasks and the status filters to focus on active, completed, failed or paused transfers. Open the inspector for details about the selected task.

## Pause, resume and retry

Pause stops the current transfer and keeps available partial data. Resume starts it again using the protocol’s resume behavior. A retry can recheck the source or remote connection. Network waits are bounded; interruption does not imply that an unresponsive remote server acknowledged the request.

For a batch selection, Fluxion processes each selected task and reports partial failures while continuing with the others. The selected count refers to the visible action targets.

## Trash and files

Move unwanted tasks to Trash and restore recoverable tasks there. Permanent deletion has a confirmation. Deleting a record and deleting its downloaded files are separate choices; review the file option before confirming.

Removing a task also removes its associated stored credentials. Keep your own source or access information if you may need to recreate the download later.

## Saved state

Task history persists locally. After a restart, inspect interrupted tasks before resuming. Saved state does not guarantee that the remote source is unchanged or that a moved download folder is still available.
