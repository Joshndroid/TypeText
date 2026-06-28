# TypeText

TypeText is a small native Rust desktop app for storing reusable text snippets and typing them into the active app. Keep snippets in simple JSON files, open the chooser with a global hotkey, search or filter by group, then insert one snippet or a queued chain of snippets.

The app is portable by design: no installer is required for portable builds, and
runtime data uses simple JSON files. Windows integration uses native operating
system APIs rather than a browser engine or web-based application runtime.

## Features

- Global hotkey to show TypeText while it runs in the background
- Searchable snippet chooser with group filtering
- Snippet chaining for inserting multiple text chunks together
- Dynamic local date and time tokens in snippet text
- Custom snippet tokens stored in `tokens.json`
- Configurable queued-snippet behavior: add duplicates or remove queued entries
- Configurable paragraph separators between queued snippets
- Built-in group and snippet editor
- DropText INI/CSV import and TypeText JSON export
- Simple JSON snippet/settings storage
- Configurable delay before typing and close-after-insert behavior
- Windows-only character and separator typing delays
- Open-on-startup setting for macOS and Windows
- Light, dark, and system theme support with configurable accent color
- Staged settings changes with a visible Save Settings reminder
- Daily GitHub release update checks with platform-specific download links
- Bundled JetBrains Mono UI font for consistent rendering
- Low-footprint native Rust/egui desktop app
- Stripped-down Windows offline-portable build with update, URL-opening, and
  startup-registration code removed at compile time

## Implementation

- Rust workspace with a shared `typetext-core` crate
- `egui/eframe` desktop UI in `apps/typetext-desktop`
- Windows support for:
  - native global hotkey registration with `RegisterHotKey`
  - native target-window restore with `SetForegroundWindow`
  - native Unicode text insertion with `SendInput`
  - configurable character and separator input delays
  - per-user startup registration through the Windows Registry
- macOS support for:
  - Carbon global hotkey registration
  - target-application restore
  - text insertion through `osascript` / System Events
  - LaunchAgent startup integration
- JSON snippet/settings storage

## Layout

```text
crates/
  typetext-core/        shared Rust library

apps/
  typetext-desktop/     desktop app

docs/
  macos-notes.md

examples/
  snippets.json
  settings.json
  settings.offline.json
  tokens.json

quickstart.txt          user-facing setup and usage handout
```

## App Data

Windows portable builds store runtime data beside the executable:

```text
TypeText.exe
data/
  snippets.json
  settings.json
  tokens.json
```

The Windows offline-portable build requires this adjacent `data` directory to
be writable and never falls back to AppData. It is intentionally lean from a
security perspective: update checking, external update URL opening, and Windows
startup-registry support are excluded at compile time, not merely hidden or
disabled in settings. The package also seeds `data/settings.json` from the
offline example, with update checks and startup opening set to false, so its
on-disk configuration accurately describes that behavior. Its dependency graph
is also checked during the build to ensure that Windows Registry support has not
been included. This reduces the
build's network-facing and persistence-related capability surface; it is not a
claim that any software is risk-free.

Offline portable mode also refuses to start with its data folder on a UNC or
mapped network drive, and refuses imports from or exports to remote Windows
storage. "Offline" means TypeText initiates no update or web traffic and keeps
its application data on local storage; Windows and other applications retain
their normal networking capabilities.

Portable snippet data is readable JSON and is not encrypted. Treat the complete
TypeText folder as private, review imported snippets before using them, and do
not store passwords, recovery codes, API keys, or other secrets in TypeText.
Anyone able to modify the data folder can alter text that TypeText will later
type. Use operating-system or full-device encryption when the storage device
needs protection at rest.

The offline build still uses the native Windows APIs required for its core job:
registering the global hotkey, restoring the target window, and inserting
Unicode text. It does not include a browser engine or web application runtime.

Installable builds use the normal per-user app data location:

```text
Windows: %LOCALAPPDATA%\TypeText\data
macOS:   ~/Library/Application Support/TypeText/data
```

## Security Features

TypeText uses several safeguards to limit its local and update-related attack
surface:

- Snippet and settings files are subject to byte-size, item-count, field-length,
  UTF-8, and numeric-range limits before their contents are used.
- Data files are saved using unique temporary files, flushed to storage, and
  atomically renamed, reducing the risk of partial or corrupted saves.
- Data-directory resolution fails closed if TypeText cannot identify a safe
  executable-adjacent or per-user location; it does not fall back to the current
  working directory.
- Windows inserts Unicode through native `SendInput` events and rechecks the
  target window before each character. macOS sends generated AppleScript to
  `osascript` over standard input, keeping snippet text out of process arguments.
- Update checks use GitHub over HTTPS. Links offered by the app must belong to
  `github.com/Joshndroid/TypeText/`, and TypeText never automatically downloads,
  executes, or installs an update.
- Release assets must include well-formed SHA-256 metadata before TypeText offers
  them. Published builds also receive GitHub provenance attestations and the
  platform checks described under [Release Security Checks](#release-security-checks).
- The Windows offline-portable build removes update, external-URL, and startup
  registration code at compile time and refuses remote data storage.

These controls reduce risk but do not make TypeText a secrets manager. Snippets
are stored as readable, unencrypted JSON; do not store passwords, recovery codes,
API keys, or other secrets in TypeText.

## Dynamic Tokens

TypeText expands supported tokens immediately before typing. Dates and times use
the computer's local time zone, and a queued snippet chain shares one timestamp.
Stored snippet text is not modified. In the snippet editor, use the **Tokens**
menu next to the title field to insert a token at the body cursor.

| Token | Output format | Example |
|---|---|---|
| `{time.today}` | Current time; legacy DropText alias | `17:42` |
| `{time.now}` | Current time | `17:42` |
| `{date.today}` | Today's date | `20/06/2026` |
| `{date.tomorrow}` | Tomorrow's date | `21/06/2026` |
| `{date.yesterday}` | Yesterday's date | `19/06/2026` |
| `{datetime.now}` | Current date and time | `20/06/2026 17:42` |
| `{weekday.today}` | Current weekday | `Saturday` |

Unknown tokens are typed unchanged. To type a supported token literally, double
the braces: `{{date.today}}` types `{date.today}`.

Custom tokens are stored in `tokens.json` beside snippets and settings. Use
Edit > Tokens to add central values such as `{program.version}` or
`{company.name}`. Updating one custom token changes every snippet that uses it
the next time TypeText types the snippet.

## Build And Run

Portable and installable releases are built natively per operating system by
the GitHub Actions workflow in `.github/workflows/release-builds.yml`.

Release artifacts:

```text
TypeText-macOS.zip
TypeText-macOS.dmg
TypeText-Windows-x64.zip
TypeText-Windows-x64-Offline-Portable.zip
TypeText-Windows-x64-Setup.exe
```

For Windows, choose the package that matches the environment:

- `TypeText-Windows-x64-Setup.exe` installs TypeText and uses the normal
  per-user AppData location.
- `TypeText-Windows-x64.zip` is the full portable build. It keeps data beside
  the executable while retaining update checks and optional per-user startup
  registration.
- `TypeText-Windows-x64-Offline-Portable.zip` is the stripped, strictly local
  variant for controlled or disconnected environments. It keeps data beside
  the executable and compiles out update checks, external update links, and
  Registry-based startup registration.

All three are native 64-bit Windows applications. They use Windows APIs for
global hotkeys, window activation, and Unicode input, with no bundled browser
engine or web application runtime.

To publish a GitHub Release, push a version tag in `vX.X.X` format:

```bash
git tag v1.4.0
git push origin v1.4.0
```

Before creating the tag, run **Build release apps** manually from the GitHub
Actions page and enter the intended `vX.Y.Z` tag. This dry run uses the real
release jobs to check formatting, tests, Clippy, the dependency audit, signed
and notarized macOS packaging, all Windows packages, and the Microsoft Defender
scan. It also verifies the final artifact set and previews the release notes,
but it does not create a tag, provenance attestations, or a GitHub Release.

The workspace `version` in `Cargo.toml` is the single source of truth for the
TypeText release version. Update it, let Cargo refresh `Cargo.lock`, commit both
files, then create a matching `vX.X.X` tag.

The workflow builds the macOS and Windows portable apps and installable packages,
then attaches them to the matching GitHub Release.
`Scripts/generate-release-notes.sh`
generates the release page changelog from commits since the previous `vX.X.X`
tag, plus a full diff link. That same version is compiled into the app UI and
written into portable build metadata. GitHub displays a SHA-256 digest for each
release asset, so separate `.sha256` files are not attached.

GitHub release artifacts also include build provenance attestations. Verify a
downloaded artifact with the GitHub CLI:

```bash
gh attestation verify TypeText-Windows-x64.zip --repo Joshndroid/TypeText
```

Attestations prove the artifact was produced by this repository's GitHub Actions
workflow.

### Release Security Checks

The release workflow must complete platform security checks before it publishes
any GitHub Release:

- The macOS app is signed with an Apple Developer ID, submitted to Apple for
  notarization, and assessed with Gatekeeper. The workflow also validates the
  notarization ticket stapled to both the app and DMG.
- Microsoft Defender scans the completed Windows portable archives, their
  extracted contents, and the Windows installer using current signatures. Any
  detection, unavailable Defender service, or incomplete scan blocks release
  publication.
- GitHub generates provenance attestations for the exact artifacts that passed
  those checks before they are attached to the release.

A successful scan means that the named security service reported no detections
at build time. It is an additional release safeguard, not a guarantee that
software can never contain or later develop a security issue.

Local builds also read their version from `Cargo.toml`.

TypeText checks that release feed at most once per day when update checks are
enabled. When a newer platform-specific package is available, the app offers a
download link and prefers installable packages over portable archives. It only
offers assets with valid SHA-256 metadata from GitHub and displays the expected
digest for verification after download. TypeText does not download, execute, or
replace the running app automatically.

For a user-facing setup guide to include with releases, see `quickstart.txt`.

Check the shared core and desktop app:

```bash
cargo test -p typetext-core
cargo check -p typetext-desktop
```

Release verification also audits `Cargo.lock` against the current RustSec
advisory database. A separate scheduled workflow repeats that audit weekly so a
new advisory can block subsequent builds even when the dependency lockfile has
not changed.

Run the desktop app during development:

```bash
cargo run -p typetext-desktop
```

### macOS

Build the portable app bundle and zip:

```bash
Scripts/build-macos-app.sh
open dist/TypeText.app
```

Build the installable DMG:

```bash
Scripts/build-macos-dmg.sh
```

Outputs:

```text
dist/TypeText.app
dist/TypeText-macOS.zip
dist/TypeText-macOS.dmg
```

### Windows

Build the portable app:

```powershell
Scripts\build-windows-portable.ps1
```

Build only one portable variant:

```powershell
Scripts\build-windows-portable.ps1 -Variant Standard
Scripts\build-windows-portable.ps1 -Variant Offline
```

The Windows build targets 64-bit Windows:

```text
x86_64-pc-windows-msvc
```

If Rust says the target is missing, install it once:

```powershell
rustup target add x86_64-pc-windows-msvc
```

Build the installer:

```powershell
Scripts\build-windows-installer.ps1
```

The installer script requires Inno Setup 6. On GitHub Actions this is installed
with Chocolatey before the script runs.

Outputs:

```text
dist\TypeText-Windows\TypeText.exe
dist\TypeText-Windows-x64.zip
dist\TypeText-Windows-x64-Offline-Portable.zip
dist\TypeText-Windows-x64-Setup.exe
dist\TypeText-Windows-x64-Setup.exe.sha256
```

The offline portable build does not check for updates, open update links, or
offer startup registration. Those Windows integrations and the Registry
dependency are excluded at compile time.

## macOS Permissions

macOS requires Accessibility permission for synthetic keyboard input.

When running from development, grant permission to the terminal app used to
launch TypeText. When running a packaged app, grant permission to TypeText
itself.

```text
System Settings > Privacy & Security > Accessibility
```

## Default Settings

```json
{
  "hotkey": "Ctrl+Alt+Space",
  "typingDelayMs": 80,
  "windowsCharacterDelayMs": 22,
  "windowsSeparatorDelayMs": 35,
  "closeAfterInsert": true,
  "startSnippetsOnNewLine": false,
  "emptyLinesBetweenSnippets": 0,
  "openOnStartup": false,
  "theme": "system",
  "accentColor": "#0A7E76",
  "queuedSnippetClickAction": "addAgain",
  "checkForUpdates": true,
  "lastUpdateCheckUnix": null
}
```

Settings changes are staged in the UI. When the Settings header shows unsaved
changes, click Save Settings to persist and apply them.

## Bundled Fonts

TypeText bundles JetBrains Mono Regular for a consistent UI font across platforms.
The font is licensed under the SIL Open Font License 1.1; see
`apps/typetext-desktop/assets/fonts/JetBrainsMono-OFL.txt`.
