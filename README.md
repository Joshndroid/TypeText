# TypeText

TypeText is a small native Rust desktop app for storing reusable text snippets and typing them into the active app. Keep snippets in simple JSON files, open the chooser with a global hotkey, search or filter by group, then insert one snippet or a queued chain of snippets.

The app is portable by design: no installer is required for portable builds, and
runtime data uses simple JSON files.

## Features

- Global hotkey to show TypeText while it runs in the background
- Searchable snippet chooser with group filtering
- Snippet chaining for inserting multiple text chunks together
- Dynamic local date and time tokens in snippet text
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

## Implementation

- Rust workspace with a shared `typetext-core` crate
- `egui/eframe` desktop UI in `apps/typetext-desktop`
- Windows support for:
  - global hotkey registration
  - target-window restore
  - text insertion with `SendInput`
  - configurable character and separator input delays
  - Startup folder integration
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

quickstart.txt          user-facing setup and usage handout
```

## App Data

Windows portable builds store runtime data beside the executable:

```text
TypeText.exe
data/
  snippets.json
  settings.json
```

Installable builds use the normal per-user app data location:

```text
Windows: %LOCALAPPDATA%\TypeText\data
macOS:   ~/Library/Application Support/TypeText/data
```

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

To publish a GitHub Release, push a version tag in `vX.X.X` format:

```bash
git tag v0.2.1
git push origin v0.2.1
```

The workflow passes the tag through as `TYPETEXT_VERSION`, builds the macOS and
Windows portable apps and installable packages, then attaches them to the
matching GitHub Release. `Scripts/generate-release-notes.sh`
generates the release page changelog from commits since the previous `vX.X.X`
tag, plus a full diff link. That same version is compiled into the app UI and
written into portable build metadata. GitHub displays a SHA-256 digest for each
release asset, so separate `.sha256` files are not attached.

GitHub release artifacts also include build provenance attestations. Verify a
downloaded artifact with the GitHub CLI:

```bash
gh attestation verify TypeText-Windows-x64.zip --repo fruitmac/TypeText
```

Attestations prove the artifact was produced by this repository's GitHub Actions
workflow.

For local builds that are not run from an exact Git tag, update `VERSION` first.
You can also override any build explicitly:

```bash
TYPETEXT_VERSION=v0.2.1 Scripts/build-macos-app.sh
```

TypeText checks that release feed at most once per day when update checks are
enabled. When a newer platform-specific package is available, the app offers a
download link and prefers installable packages over portable archives; it does
not replace the running app automatically.

For a user-facing setup guide to include with releases, see `quickstart.txt`.

Check the shared core and desktop app:

```bash
cargo test -p typetext-core
cargo check -p typetext-desktop
```

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

The offline portable build does not check for updates or offer startup
registration. Those Windows integrations are disabled at compile time.

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
