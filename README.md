# TypeText

Version: `0.1.0`

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

## Portable Data

At runtime, TypeText stores data beside the executable:

```text
TypeText.exe
data/
  snippets.json
  settings.json
```

The same layout is used for packaged builds, so TypeText can be moved as a folder without installing it.

## Build And Run

Portable release archives are built natively per operating system. The GitHub
Actions workflow in `.github/workflows/build-portable.yml` builds and uploads:

```text
TypeText-macOS.zip
TypeText-Windows-x64.zip
TypeText-Linux-<target>.tar.gz
```

To publish a GitHub Release, push a version tag in `v.X.X.X` format:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow builds the macOS, Windows, and Linux portable archives, then
attaches them to the matching GitHub Release.

Check the shared core and desktop app:

```bash
cargo test -p typetext-core
cargo check -p typetext-desktop
```

Run the desktop app during development:

```bash
cargo run -p typetext-desktop
```

Build a portable macOS app bundle:

```bash
Scripts/build-macos-app.sh
open dist/TypeText.app
```

The macOS portable archive is written to:

```text
dist/TypeText-macOS.zip
```

Build the portable Windows app:

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

The portable Windows output is written to:

```text
dist\TypeText-Windows\TypeText.exe
```

That folder also includes `data\`, `TypeText.ico`, and `build-info.txt`.
The Windows portable archive is written to:

```text
dist\TypeText-Windows-x64.zip
```

Build the portable Linux app:

```bash
Scripts/build-linux-portable.sh
```

The portable Linux output is written to:

```text
dist/TypeText-Linux/TypeText
```

The Linux portable archive is written to:

```text
dist/TypeText-Linux-<target>.tar.gz
```

Linux currently uses the shared Rust UI with fallback platform hooks. Global
hotkey and synthetic typing support are not implemented on Linux yet.

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
  "queuedSnippetClickAction": "addAgain"
}
```

## Bundled Fonts

TypeText bundles JetBrains Mono Regular for a consistent UI font across platforms.
The font is licensed under the SIL Open Font License 1.1; see
`apps/typetext-desktop/assets/fonts/JetBrainsMono-OFL.txt`.
