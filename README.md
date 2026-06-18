# TypeText

Version: `v0.2.1`

TypeText is a small native Rust desktop app for storing reusable text snippets and typing them into the active app. Keep snippets in simple JSON files, open the chooser with a global hotkey, search or filter by group, then insert one snippet or a queued chain of snippets.

The app is portable by design: no installer is required, and runtime data lives beside the executable in a local `data/` folder.

## Features

- Global hotkey to show TypeText while it runs in the background
- Searchable snippet chooser with group filtering
- Snippet chaining for inserting multiple text chunks together
- Configurable queued-snippet behavior: add duplicates or remove queued entries
- Built-in group and snippet editor
- DropText INI import and TypeText JSON export
- Portable JSON data files stored beside the executable
- Configurable typing delay and close-after-insert behavior
- Open-on-startup setting for macOS and Windows
- Light, dark, and system theme support
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
  - Startup folder integration
- macOS support for:
  - Carbon global hotkey registration
  - target-application restore
  - text insertion through `osascript` / System Events
  - LaunchAgent startup integration
- Linux/Ubuntu support for:
  - XDG Desktop Portal global shortcut registration on Wayland
  - X11 global hotkey registration on Xorg sessions
- JSON snippet/settings storage

## Layout

```text
crates/
  typetext-core/        shared Rust library

apps/
  typetext-desktop/     desktop app

docs/
  macos-notes.md
  linux-notes.md

examples/
  snippets.json
  settings.json
```

## App Data

Portable builds store runtime data beside the executable:

```text
TypeText.exe
data/
  snippets.json
  settings.json
```

Installable builds first try that portable `data/` folder, then fall back to the
normal per-user app data location when the install directory is not writable:

```text
Windows: %LOCALAPPDATA%\TypeText\data
macOS:   ~/Library/Application Support/TypeText/data
Linux:   $XDG_DATA_HOME/typetext/data or ~/.local/share/typetext/data
```

## Build And Run

Portable and installable releases are built natively per operating system by
the GitHub Actions workflow in `.github/workflows/build-portable.yml`.

Release artifacts:

```text
TypeText-macOS.zip
TypeText-macOS.dmg
TypeText-Windows-x64.zip
TypeText-Windows-x64-Setup.exe
TypeText-Linux-<target>.tar.gz
typetext_<version>_amd64.deb
```

To publish a GitHub Release, push a version tag in `vX.X.X` format:

```bash
git tag v0.2.1
git push origin v0.2.1
```

The workflow passes the tag through as `TYPETEXT_VERSION`, builds the macOS,
Windows, and Linux portable archives and installable packages, then attaches
them to the matching GitHub Release. That same version is compiled into the app
UI and written into portable build metadata.

For local builds that are not run from an exact Git tag, update `VERSION` first.
You can also override any build explicitly:

```bash
TYPETEXT_VERSION=v0.2.1 Scripts/build-linux-portable.sh
```

TypeText checks that release feed at most once per day when update checks are
enabled. When a newer platform-specific package is available, the app offers a
download link and prefers installable packages over portable archives; it does
not replace the running app automatically.

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
dist\TypeText-Windows-x64-Setup.exe
```

### Linux

Build the portable app:

```bash
Scripts/build-linux-portable.sh
```

Build the DEB package:

```bash
Scripts/build-linux-deb.sh
```

Outputs:

```text
dist/TypeText-Linux/TypeText
dist/TypeText-Linux-<target>.tar.gz
dist/typetext_<version>_amd64.deb
```

Linux currently uses the shared Rust UI with XDG Desktop Portal global shortcut
support on Wayland and X11 global hotkey support on Ubuntu/Xorg sessions.
Synthetic typing support is not implemented on Linux yet.

## macOS Permissions

macOS requires Accessibility permission for synthetic keyboard input.

When running from development, grant permission to the terminal app used to launch TypeText. When running a packaged app, grant permission to TypeText itself.

```text
System Settings > Privacy & Security > Accessibility
```

## Default Settings

```json
{
  "hotkey": "Ctrl+Alt+Space",
  "typingDelayMs": 80,
  "closeAfterInsert": true,
  "openOnStartup": false,
  "theme": "system",
  "queuedSnippetClickAction": "addAgain",
  "checkForUpdates": true,
  "lastUpdateCheckUnix": null
}
```

## Bundled Fonts

TypeText bundles JetBrains Mono Regular for a consistent UI font across platforms.
The font is licensed under the SIL Open Font License 1.1; see
`apps/typetext-desktop/assets/fonts/JetBrainsMono-OFL.txt`.
