# Fluxion Releases

The Rust/GPUI app uses signed native macOS update archives, with separate Stable
and Beta feeds at `https://assets.fluxion.alkinum.io`.

## Channels and client behavior

- Stable tags: `v1.2.3`; only final releases enter the Stable feed.
- Beta tags: `v1.2.3-beta.4`; only `-beta.N` releases enter the Beta feed.
- Settings stores the selected channel and automatic-check preference in
  `native-ui.json`. A new installation defaults to its build channel.
- Automatic checks run at launch and every six hours. Checks never interrupt a
  transfer or open a modal. Available updates appear in the sidebar and Settings.
- Beta users receive the newest eligible Beta or Stable release. A final release
  supersedes its beta. Switching to Stable never downgrades; the app waits for a
  newer final release. Equal versions and build-metadata-only changes are ignored.
- Download and restart are separate actions. The app stays usable while the
  update downloads and is verified. Restart pauses transfers and waits for Core
  to flush before replacing the application.

```text
releases/stable/<version>/...
releases/beta/<version>/...
updates/stable/latest.json
updates/beta/latest.json
```

Manifest `channel`, version and artifact path must agree. The client also accepts
older manifests without an explicit channel when the version and feed agree.
Universal archives are listed under both `darwin-aarch64` and `darwin-x86_64`.

## Repository secrets

The workflow requires these repository secrets and fails before building if any
required value is missing. Values must never be committed to this repository.

| Secret | Purpose |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 Developer ID Application P12 certificate and private key |
| `APPLE_CERTIFICATE_PASSWORD` | P12 import password |
| `APPLE_SIGNING_IDENTITY` | Full Developer ID Application identity name |
| `APPLE_API_KEY` | App Store Connect team API private key in P8 format |
| `APPLE_API_KEY_ID` | ID of that notarization API key |
| `APPLE_API_ISSUER` | Issuer UUID for that team API key |
| `TAURI_SIGNING_PRIVATE_KEY` | Existing Minisign update key matching the embedded public key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Update-key password, optional if unencrypted |
| `CLOUDFLARE_API_TOKEN` | R2 object read/write token |

The Cloudflare account and bucket name are non-secret workflow configuration.
The Tauri signer is only a standalone release tool to preserve update-key
compatibility. No Tauri runtime or web frontend ships in the app. Do not rotate
the update key without a client migration: existing apps trust the public key in
`desktop/native/updater.rs`.

## Build and publish

```sh
python3 scripts/bundle-macos.py --release --universal --dmg --notarize \
  --version 1.2.3-beta.4 --channel beta
```

GitHub Actions imports the Developer ID certificate into a temporary keychain,
builds both architectures, signs the universal application, notarizes and staples
the app, packages the updater archive, then signs/notarizes/staples the DMG. The
notarization private key lives only in a protected temporary file, not process
arguments. The updater archive is signed separately with the existing Minisign
key. Failed signing, notarization, verification or tests prevent publication.

Versioned artifacts upload before the channel manifest. Release jobs are serialized
and publishing checks that the channel pointer will advance, rejecting an equal
or older version. Stable and Beta pointers are independent. Pushing ordinary
commits does not publish an app; only supported tags or workflow dispatch do.
Existing versioned assets are never overwritten. If an upload is interrupted,
inspect the partial prefix before retrying or publish a new version; the preflight
rejects any existing artifact rather than changing an immutable cached URL.

Local `--preview` and builds without `--notarize` may use ad-hoc signatures. These
are for development and must not be described as notarized public releases.

## Installer guarantees and limits

The client allows only HTTPS artifacts on the release origin, rejects redirects,
limits manifest/archive size and verifies the Minisign signature before extraction.
Extraction accepts only regular files/directories under `Fluxion.app`, with no
links, traversal or special entries. It verifies macOS code signatures, the bundle
identifier, executable and expected version. A digest detects any changes to the
staged bundle before installation.

The application directory must be writable. Updates are staged beside the app so
replacement uses same-volume renames. The helper waits at most 90 seconds for the
old process to exit; timeout leaves the installation untouched. A failed rename
or failed launch command restores the previous app. The previous bundle remains
in `.fluxion-update-*/previous.app` for manual recovery. A successful `open` command
is not proof of subsequent process health; crashes after launch require manual
recovery. Pending downloads/staging are discarded when the app exits without
choosing installation or the user changes channels.

## Verification

```sh
cargo test --workspace --locked
node --test scripts/release/*.test.mjs
python3 -m unittest discover -s scripts/release -p 'test_*.py'
python3 scripts/bundle-macos.py --preview
```

Tests cover channel isolation and promotion, no downgrade, stale check results,
periodic preferences, signature validity/tampering, archive restrictions, staged
bundle changes, manifest validation, monotonic publishing, replacement/launch
rollback and exit timeout. Helper tests use temporary applications and a mock
launcher; they do not install a real release. A live, Developer ID signed update
and Gatekeeper check still require configured release credentials and a published
test release.
