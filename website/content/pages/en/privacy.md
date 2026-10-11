---
title: "Privacy policy"
description: "How Fluxion handles local tasks, network transfers and website information."
---

**Effective: 11 October 2026.** This policy describes the current Fluxion development app and this website. The app has not been released on the Mac App Store. Publisher identity and a private contact channel remain to be confirmed before app submission. Check [release status](/download) for distribution availability.

## Information on your Mac

Fluxion stores task history, settings, transfer state and downloaded files locally. The Mac App Store build stores its application data in its sandbox container; access to other download folders is granted when you choose them. Persistent file-access bookmarks are stored locally.

Sensitive HTTP headers, signed source URLs and transfer credentials are stored in macOS Keychain. The task database stores references to these secrets. Private-key authentication reads the key file you select; Fluxion does not upload your private key to the developer. Removing a task removes its associated stored credentials. Downloaded files are deleted only when you explicitly choose that option in the confirmation.

Task history and settings persist until removed. Removing the app may leave downloaded files and some local data or Keychain items behind. Diagnostics are redacted, but you should still review any export or screenshot before sharing it.

## Network connections needed for transfers

Downloads connect to the servers you choose. Those servers can receive your IP address, requested path, request headers, credentials required by the protocol and normal connection metadata. Proxy services receive relevant connection metadata when you use them. FTP is not encrypted; prefer HTTPS or SFTP when transmitting sensitive information.

BitTorrent connects to trackers, DHT nodes and peers. They can see your IP address and torrent identifiers. Pieces may be uploaded to peers during downloading, and seeding can continue uploading after completion if enabled. Peer/client filtering is a connection policy, not anonymity or proof of a peer’s identity. Remote services have their own privacy practices.

## Developer services

The app has no developer-operated analytics, advertising, user account system or cloud task synchronization. It does not upload download history, files or transfer credentials to the developer.

The Mac App Store edition receives updates through Apple. The direct edition can check the developer’s update host for signed releases; that host receives ordinary HTTP metadata such as IP address and app/version request information. Optional browser capture belongs to the direct edition and asks you to review each imported task.

## This website

Cloudflare serves this website and processes ordinary request metadata, including IP address and request headers, to deliver and protect it. We do not add analytics, advertising trackers or external fonts. The theme preference can be saved in browser local storage. Documentation search runs locally in the browser; queries are not sent to an AI service. Clear site data in your browser to remove saved preferences.

## Support

The OnFire support form is **not currently enabled**. This site does not currently collect support email addresses or submit tickets. Do not send private information through the public issue tracker.

When the form is enabled, this policy will be updated to describe its operation. The reserved form sends your email, message, selected topic, language and optional app/macOS/protocol details to the OnFire service at support.alkinum.io. Cloudflare Turnstile would process verification information to prevent abuse, and the intake would use IP-based rate limits. This processing does not happen through the disabled form.

## Questions and changes

Current contact availability is listed on the [support page](/support). A private channel for access, correction or deletion requests concerning support records must be operational before app release.

We will update this page when data handling or support availability changes. The effective date identifies the current version.
