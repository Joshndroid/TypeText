# Linux Notes

TypeText uses the shared Rust/egui desktop window on Linux. The current Linux
build targets Ubuntu and is packaged as a portable experiment so the UI and
data model can be tested on target distributions.

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

- Global hotkey support uses the XDG Desktop Portal GlobalShortcuts API on
  Wayland and X11 global key grabs on Xorg.
- Wayland support depends on the active desktop portal backend. On unsupported
  desktops TypeText will show a hotkey availability error at startup.
- Synthetic typing is not implemented on Linux yet.
- Some desktop environments restrict synthetic input, especially under Wayland.
- Building from source on Ubuntu requires the X11 development package, typically
  `libx11-dev`, in addition to the usual Rust desktop build dependencies.
- Linux should be treated as a supported experiment until tested on target
  distributions.
