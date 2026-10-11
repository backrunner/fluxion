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
- `crates/fluxion-browser`: Native Messaging host and the private browser/App bridge.
  The existing Core owns browser tasks, so they arrive through the same UI events.
  See [browser setup](browser-extension.md).
- `desktop/native/model.rs`: event reduction, filtering, formatting and UI preferences.
- `desktop/native/updater.rs`: channel selection, signed downloads, staged installation and restart.
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
Command-I. Task progress, speed, state and list changes update automatically through
Core events; a lagged event stream automatically resynchronizes its snapshot.
With the task list focused, up/down moves selection,
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
node --test scripts/release/*.test.mjs
python3 -m unittest discover -s scripts/release -p 'test_*.py'
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

## Interface conventions

Keep the three-pane workspace, 84 px task rows, 20 px action glyphs and compact
30–36 px controls. Use spacing and a small number of tonal surfaces to separate
navigation, list and inspector. Avoid repeated nested cards, decorative icon
badges, oversized empty states and numerical columns that displace filenames.
Progress belongs to task information; actions form a separate group. Detail
actions stay in the header while the body scrolls. Copy controls sit beside the
information they copy. Settings uses one scroll container and clearly separates
immediate UI/update preferences from explicitly saved transfer settings. Keep
units with field labels. Destructive confirmation titles remain short; selected
counts belong in their bodies.

The list header, search/sort bar, filters and batch controls share 24 px side
insets. Header buttons use 32 px targets. Search and sort share one 32 px surface;
the sort menu is separated from the input by a short vertical divider.
Leave 12 px between the search bar and filters and above/below the 36 px batch
control group. Separate the delete action inside that group with a short divider.

The detail title keeps the file icon, name and size on one line, with a compact
protocol tag aligned to the right. Reserve metadata width before truncating long
names so the size and tag remain visible in narrow panes.

In file details, keep the redacted source and its copy button in one compact row.
Long sources use a single line with an ellipsis; copying retains the complete
redacted source. Show creation time as a label/value row in the UI locale and
the local time zone.

Shared control geometry and settings groups live in `desktop/native/ui_style.rs`.
Keep the existing orange accent and cool neutral palette. Use 13 px field labels,
subtle input borders and shadows, and a distinct selected segment in both themes.
Settings groups fill the available pane width with equal 28 px side insets;
labels remain on the left and switches on the right. The update footer pairs
version/status information with actions on the right. Privacy and support links
align with their description and end with an external-link glyph. Custom request
headers and trackers are directly editable; password and cookie inputs remain
masked. Settings still has one scroll container and
a fixed save action for transfer preferences.

Software updates are described in [releasing.md](releasing.md).

The 2026-10-11 interface refinement passed `cargo check -p fluxion-app --all-targets`,
22 default desktop tests, 13 App Store feature tests, formatting and diff checks.
The existing GPUI layout test exercises settings at 760 and 1180 px in both
themes and all four locales. The isolated native preview was also inspected in
light/dark appearance and a zoomed window, including settings scrolling,
protected-field disclosure, input focus, and the new-download validation error.
Narrow-window coverage in this pass is from the layout test, not a manual resize.

The 2026-10-12 review also verified 27 default desktop tests and 18 App Store
feature tests. Task names now reserve row controls before shaping an ellipsis
and retain the full name in a tooltip. Native preview checks covered narrow and
wide task columns and multi-selection. Checkboxes appear only in selection mode;
the file icon stays beside the title while the status row shares its left edge.
Inspector actions sit in the fixed header beside the collapse control. Advanced
settings and rate limits share animated disclosure cards; all workspace scroll
containers use measured edge fades.
