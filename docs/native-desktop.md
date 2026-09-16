# Native desktop

Fluxion now renders its complete desktop interface with GPUI 0.2.2 and
gpui-component 0.5.1. The Svelte, Vite, Node frontend and Tauri runtime have been
removed. The navigation / task list / inspector structure is preserved.

## Code boundaries

- `desktop/native/ui.rs`: workspace state, commands, keyboard navigation and dialogs.
- `desktop/native/ui/`: virtual task list, sidebar, inspector and settings views.
- `desktop/native/form.rs`: native inputs, validation, per-protocol task creation,
  limits, trackers, file selection and system file dialogs.
- `desktop/native/backend.rs`: bounded command/event channels and asynchronous
  calls into Fluxion Core. Filesystem and network operations run on Tokio.
- `desktop/native/model.rs`: event reduction, filtering, formatting and UI preferences.
- `desktop/native/updater.rs`: compatible signed native updates.
- `desktop/assets`: embedded Lucide SVGs, app icons and four language dictionaries.

The UI does not import protocol engines. Progress uses Core events; snapshots load
initial state and recover from dropped events. Selected BT state is refreshed at
most once per second while events arrive. The virtual list renders visible rows.

## Existing data

Production storage remains at
`~/Library/Application Support/top.backrunner.fluxion/core/fluxion.sqlite`, using
the same Keychain service. No task database is deleted or replaced by the UI
migration. On first production launch, three non-secret preferences are copied
read-only from the old app's WebKit local storage: theme, language and trash IDs.
WebKit files remain untouched. Native UI preferences are written atomically to
`native-ui.json`; request headers, passwords and signed URLs are never saved there.

`FLUXION_DATA_DIR` overrides the storage directory and disables legacy preference
import. The preview bundle sets it to `/tmp/fluxion-gpui-ui-check` and uses a
different bundle identifier. This keeps testing separate from production tasks.

## Interaction

New task: Command-N. Search: Command-F. Settings: Command-comma. Inspector:
Command-I. Refresh: Command-R. With the task list focused, up/down moves selection,
Space pauses/resumes, Command-A selects visible tasks and Backspace opens the
delete confirmation. Input fields retain their native editing shortcuts.

Task actions follow Core lifecycle rules. New tasks enter the queue, then Start
begins the transfer. Trash stops an active transfer and remains recoverable until
the user confirms permanent deletion; deleting local files requires a separate
unchecked option. Below 1020 px the inspector overlays the task area, with the
sidebar and close control remaining available.

## Validation

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
node --test scripts/release/create-update-manifest.test.mjs
python3 scripts/bundle-macos.py --preview
```

Native tests cover validation, secret isolation, event reduction, filter and trash
semantics, update archive rejection, and a real local HTTP transfer through the
native backend with content and persistence verification. Visual and interaction
checks must additionally use the actual macOS application.

## Verification recorded during migration

On macOS, the actual preview app was exercised with an isolated 32 MiB local HTTP
fixture: create with a Chinese filename, start, pause, restart and complete.
The output size and SHA-256 matched the source. This server ignores Range, so this
manual check verifies single-connection fallback, not partial HTTP resume. The
workspace tests separately cover ranged transfers and resume.

Task and theme/language preferences survived an app restart. Light/dark themes,
language changes, settings-save feedback, confirmation cancellation, empty-link
validation, and the narrow-window inspector/form were inspected in the native app.
Root dialog redraw and pre-window snapshot delivery have regression tests. The
narrow inspector blocks clicks from reaching the task list beneath it; its close
control was verified to return to the list.

The workspace test run passed 53 tests; release-manifest tests passed 2 tests.
Live BT/FTP/SFTP transfers and signed update installation were not manually
exercised. Universal release signing and notarization require the release host.
