# Mac App Store readiness

Updated: 2026-10-12. Status: **technical preparation in progress; not ready to submit**.

## Distribution policy

Apple requires Mac App Store apps to use App Sandbox and App Store updates.
Fluxion has two explicit build paths:

| Capability | Direct build | `app-store` build |
| --- | --- | --- |
| HTTP/HTTPS, BT, FTP, SFTP | Core engines | Same Core engines |
| Updates | Signed native updater | Apple-managed; updater client and installer excluded |
| Browser capture | Native Messaging registration | No host packaging, registration or bridge listener |
| Data | Existing application directory | App Sandbox container via `NSHomeDirectory` |
| Download directory | User filesystem permissions | Downloads entitlement or user-selected access |
| Persistent external access | Ordinary permissions | Security-scoped bookmarks, balanced start/stop |
| Finder / system proxy | Public native APIs | Public native APIs; no shell helpers |

The network server entitlement supports incoming BT/DHT peer traffic. It does not
enable a CLI daemon or browser service in the store app. No temporary sandbox
exceptions, broad home-directory access, JIT exception or debugging entitlement
are requested. Keep the existing direct application database intact; the store
edition does not scan WebKit or migrate data from other containers.

## Build and validation

```sh
python3 scripts/bundle-macos.py --distribution app-store
python3 scripts/check-app-store.py target/native/debug/app-store/Fluxion.app
cargo test -p fluxion-app --features app-store --locked
```

The first command makes an ad-hoc signed development bundle with sandbox
entitlements. It deliberately refuses DMG, direct notarization, beta channels,
preview identity and skipped compilation so a direct executable cannot be
mistaken for a store executable.

For a distribution candidate, provide `APPLE_SIGNING_IDENTITY` with an Apple
Distribution / Mac App Distribution identity and a profile matching the bundle
ID, then build:

```sh
python3 scripts/bundle-macos.py --release --universal --distribution app-store \
  --version 1.0.0 --build-number 1 --provisioning-profile /path/to/store.provisionprofile
python3 scripts/check-app-store.py \
  target/universal-apple-darwin/release/app-store/Fluxion.app --submission
```

The checker validates bundle metadata, actual signed entitlements, packaged
privacy/license documents, absence of browser helpers and direct updater code,
and (with `--submission`) distribution signing, a profile and both architectures.
It validates profile expiration, bundle/team identity and Keychain entitlements,
but does not replace Apple's validation or verify the entire certificate chain. A signed installer package, upload and TestFlight processing
are separate remaining steps. CI now runs store tests and packages/checks a
universal ad-hoc bundle; check its remote results for the submitted commit.

## Product audit and fixes

- A batch action previously aborted after the first failed task. It now processes
  the remaining tasks and reports the partial failure; a single task still shows
  its original error.
- SFTP previously trusted a first-use host even when its fingerprint could not
  be read/saved. It now refuses that connection, writes pins atomically with
  private permissions, and locks across threads/processes. Changed host keys
  remain rejected. First-use trust is still TOFU, not out-of-band identity proof.
- BT cleanup now runs even when transfer/progress persistence fails. A second
  active task for the same torrent is rejected instead of sharing a handle that
  either task could stop.
- BT peer-policy helpers were never wired into inbound/outbound connections.
  A documented, pinned librqbit source patch now checks IP rules before outgoing
  dialing, and checks incoming torrent/IP/Peer ID before replying to the handshake.
  Extended client names are checked before live peer handling. Magnet metadata
  connections use the same policy. Global rules and trackers are merged into
  new/restarted tasks and previews. A nonempty allow list is exclusive, with
  allow overriding deny. Client identity is self-reported and can be spoofed.
  Rapid-disconnect behavior blocking remains explicitly unsupported.
  Magnet-start metadata waits are bounded/cancellable; download/seeding polling
  wakes immediately on cancellation. Core owns task recovery; librqbit DHT
  persistence is disabled to avoid an independent home-directory cache outside
  store file authority.
- FTP/SFTP connection, authentication, metadata and read waits now have 30-second
  time bounds and observe cancellation. Partial files are flushed on interrupted
  reads. SFTP pins are located in the runtime data directory, including the store
  container, instead of an independently environment-derived location.
- Settings fields use consistent column widths/alignment, task-list insets
  align, selected counts refer to the visible action targets, download ETA is
  shown when available, and detail actions share the fixed inspector header.
  The save control is fixed in the settings header so it remains accessible
  after scrolling; actual clicking confirms successful saving.
  Empty/unselected lists use the full task area. Long filenames are truncated
  within the inspector rather than overflowing.
- The store app includes localized privacy/support controls and bundled privacy,
  project license and generated dependency notices, including the modified
  librqbit dependency. The public policy and support pages are now available at
  https://fluxion.alkinum.com/privacy/ and https://fluxion.alkinum.com/support/.
  Publisher identity and an operational private support contact remain release
  gates. The user explicitly deferred OnFire setup: both frontend and intake
  are disabled; no product credentials or real tickets were created.

## Evidence from this session

- `cargo test --workspace --locked --offline`: 93 tests passed, no failures or
  ignored Rust tests, including local HTTP integration and storage/lifecycle
  checks. Controlled loopback FTP, SSH/SFTP and BitTorrent peers verify exact
  bytes, FTP REST and SFTP private-key resume, bad credentials, changed SFTP
  fingerprints, stalled-read pause, torrent/magnet transfer, piece repair and
  incoming/outgoing IP/Peer ID/extended-client rejection and cancellation during
  a stalled real metadata handshake with no managed-torrent leak. No public swarm or
  remote FTP/SFTP endpoint was used.
- 18 store app tests pass independently, including refusal of all direct updater
  operations. Formatting and workspace/all-target compilation are checked.
- 8 Python release/packaging tests pass. Node tests: 17 pass, 1 opt-in real Chromium
  capture end-to-end test is skipped. The store edition excludes browser capture.
- Native ad-hoc sandbox bundle starts and creates its database inside its own
  container. Its actual signed entitlements pass `check-app-store.py`.
- Through the real UI, selected an external QA folder, downloaded a 2 MiB local
  HTTP fixture, restarted the app, then revealed the file in Finder without
  selecting the folder again. The downloaded file matches the fixture byte for
  byte; SHA-256:
  `91d3beb88a9b2f778a6c44a1c53b63d3c79931845a9aef84b3fb414610bd1938`.
  The saved bookmark file uses mode `0600`.
- Actual native light/Chinese and dark/English windows were inspected. Narrow
  windows are covered by GPUI layout tests; manual resizing was not established.
  These checks are development evidence, not signed-store/TestFlight QA.

## Official website

The fully customized svedocs 0.2.3 site uses English/Chinese static pages, local
search, genuine brand assets and a labeled interactive illustration, with
Cloudflare Worker `fluxion-site` on `fluxion.alkinum.com`. It does not advertise an
App Store purchase or approval. Both languages include privacy/support/license
pages and protocol/permission/troubleshooting guides.

Website validation: type checks, strict translation/link checks (22 pages,
102 search records), Worker checks, seven mocked support-boundary tests,
prerender/metadata/asset verification, deployment dry-run and real custom-domain
deployment. Public HTTP probes verify both languages, policy/support/license
assets, discovery endpoints, correct 404, and disabled-intake 503. Real browser
checks cover desktop/mobile, light/dark, search and locale navigation. The frontend does
not render support fields or load Turnstile while disabled; the API returns 503
without contacting OnFire even if credentials are accidentally supplied.
OnFire product setup and email delivery were deliberately deferred, not verified.

## Remaining submission gates

1. Verify BT selected-file behavior, seeding, share limits, pause and shutdown in
   the final signed sandbox app. Controlled protocol filtering tests are not
   public-swarm/router/UPnP validation. Rapid-disconnect behavior detection and
   automatic router mapping are not first-release claims.
2. Repeat the now-tested FTP/SFTP wire cases in the final signed sandbox app,
   including credential access, SFTP key selection and sandbox restart. Verify
   Keychain access with the final distribution team/profile. Validate Finder,
   permission revocation, stale/moved bookmarks and pause/restart for large files.
3. Produce a universal distribution build and installer package; run Apple's
   validation/upload and test the processed TestFlight build on both supported
   Mac architectures and the declared minimum macOS version.
4. Complete publisher identity and a working private support contact, enable
   and verify the deferred OnFire service, and approve App Store privacy answers.
   The dated public HTTPS policy is published, but contact completeness is pending. The bundled manifest records no tracking/collected data and reasons for file metadata,
   disk-space handling and elapsed timing; inspect the final binary/SDK report
   before treating it as a final privacy assessment.
5. Review third-party notices and source availability obligations. The locked
   runtime/build dependency inventory includes MPL-2.0 `option-ext` and build
   tool `cbindgen`; generation of notices is not legal clearance. Review fonts,
   artwork, dependency updates and the release archive as well.
6. Decide price, age rating, territories, content rights and encryption export
   answers. HTTP TLS and SFTP SSH encryption exist. Do not invent legal/export
   declarations from a successful build. Store screenshots must show the final
   store build with licensed fixture content.

## Primary references

- [Apple App Review Guidelines, including 2.4.5 and 5.1.1](https://developer.apple.com/app-store/review/guidelines/)
- [Configuring the macOS App Sandbox](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox)
- [Security-scoped bookmarks](https://developer.apple.com/documentation/foundation/nsurl/bookmarkcreationoptions/withsecurityscope)
- [Required reason API declarations](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitypereasons)
