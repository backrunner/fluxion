---
title: "Release status"
description: "Fluxion is preparing for its first stable release."
---

## Preparing for the first release

Fluxion is under active development. **It is not currently available on the Mac App Store.** There is no App Store purchase or TestFlight invitation available from this website yet.

We are preparing a sandboxed Mac App Store edition alongside a direct edition. Release availability, price and supported macOS versions will be published here once confirmed. Do not rely on an unannounced launch date.

## Two distribution paths

| Capability | Mac App Store edition | Direct edition |
| --- | --- | --- |
| HTTP, HTTPS, BitTorrent, FTP, SFTP | Shared transfer engines | Shared transfer engines |
| App updates | Through Apple | Signed native updates |
| Download folder access | App Sandbox and user-selected folders | Normal filesystem permissions |
| Browser capture | Excluded | Optional Chromium extension and local handoff |
| CLI and daemon | Separate from the store app | Available from source |

FTPS is not supported. Browser capture is not part of the planned store edition. A development build is not an Apple-reviewed release.

## Follow development

The [source repository](https://github.com/backrunner/fluxion) contains build instructions, changes and the issue tracker. A development build may have changing storage formats or protocol behavior.

See the [getting-started guide](/docs/downloading) and [protocol details](/docs/protocols) for current behavior.
