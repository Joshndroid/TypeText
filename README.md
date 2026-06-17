# TypeText

Version: `0.1.0`

TypeText is a lightweight Rust remake inspired by the old `DropText.exe` style utility: keep reusable text chunks in a simple file, open a small chooser, select a chunk, and have it typed into the active text field.

The goal is a portable Windows-first app. No installer is required, and user data lives beside the executable in a local `data/` folder.

## Features

- Global hotkey to open TypeText while it runs in the background
- Searchable snippet chooser with group filtering
- One-click snippet queuing for chaining multiple text chunks
- Optional re-click behavior to add duplicates or remove queued snippets
- Built-in snippet and group editor
- Portable JSON data files stored beside the executable
- Light, dark, and system theme support
- Bundled JetBrains Mono UI font for consistent rendering
- Low-footprint native Rust/egui desktop app

## Current Implementation

- Rust workspace
- Shared `typetext-core` library for snippets, settings, validation, search, and portable paths
- `egui/eframe` desktop UI
- Windows platform module for:
  - global hotkey registration
  - text typing with `SendInput`
- macOS test frontend using the same Rust UI:
  - global hotkey registration
  - text typing through `osascript` / System Events
- JSON snippet/settings files

## Layout

```text
crates/
  typetext-core/        shared Rust library

apps/
  typetext-desktop/     desktop app, Windows first

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

This means the app can be moved as a folder without installing it.

## Build

On macOS, this checks the shared core and UI shell:

```bash
cargo test -p typetext-core
cargo check -p typetext-desktop
```

Run the macOS test frontend:

```bash
cargo run -p typetext-desktop
```

Build a portable macOS app bundle:

```bash
Scripts/build-macos-app.sh
open dist/TypeText.app
```

To test insertion on macOS:

1. Open TypeText with `open dist/TypeText.app`.
2. Open TextEdit or another text field.
3. Return to TypeText, double-click a snippet or select one and press Enter.
4. If typing fails, grant Accessibility permission to TypeText.

Accessibility path:

```text
System Settings > Privacy & Security > Accessibility
```

On Windows, build the portable app:

```powershell
Scripts\build-windows-portable.ps1
```

The Windows portable build explicitly targets 64-bit Windows:

```text
x86_64-pc-windows-msvc
```

If Rust says the target is missing, install it once:

```powershell
rustup target add x86_64-pc-windows-msvc
```

The portable output will be under:

```text
dist\TypeText-Windows\TypeText.exe
```

That folder also includes `data\`, `TypeText.ico`, and `build-info.txt`.

## Default Settings

```json
{
  "hotkey": "Ctrl+Alt+Space",
  "typingDelayMs": 80,
  "closeAfterInsert": true,
  "theme": "system",
  "queuedSnippetClickAction": "addAgain"
}
```

## Bundled Fonts

TypeText bundles JetBrains Mono Regular for a consistent UI font across platforms.
The font is licensed under the SIL Open Font License 1.1; see
`apps/typetext-desktop/assets/fonts/JetBrainsMono-OFL.txt`.

## Notes

The original reference file at `/Users/fruitmac/Downloads/DropText.exe` is a 32-bit Windows GUI executable around 304 KB. It cannot be run in this macOS workspace, so TypeText follows the described behavior rather than cloning internals.

## MVP Goal

1. Focus a text field in any Windows app.
2. Press the configured hotkey.
3. Pick a saved text chunk from TypeText.
4. TypeText hides and types the selected chunk into the original text field.
