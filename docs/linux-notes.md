# Linux Notes

TypeText uses the shared Rust/egui desktop window on Linux. The current Linux
build is packaged as a portable experiment so the UI and data model can be
tested on target distributions.

## Portable Build

Build the Linux portable folder and archive on Linux:

```bash
Scripts/build-linux-portable.sh
```

Output:

```text
dist/TypeText-Linux/TypeText
dist/TypeText-Linux-<target>.tar.gz
```

## Known Limitations

- Global hotkey support is not implemented on Linux yet.
- Synthetic typing is not implemented on Linux yet.
- Some desktop environments restrict global hotkeys or synthetic input,
  especially under Wayland.
- Linux should be treated as a supported experiment until tested on target
  distributions.
