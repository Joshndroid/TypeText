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
- Synthetic typing uses the XDG Desktop Portal RemoteDesktop API on Wayland.
  The desktop will show a permission prompt the first time TypeText needs
  keyboard input control.
- X11/Xorg synthetic typing uses `xdotool`.
- Wayland support depends on the active desktop portal backend. On unsupported
  desktops TypeText will show a hotkey or typing availability error.
- Some desktop environments restrict synthetic input, especially if their
  portal backend does not expose keyboard input through RemoteDesktop.
- Building from source on Ubuntu requires the X11 development package, typically
  `libx11-dev`, in addition to the usual Rust desktop build dependencies.
- Linux should be treated as a supported experiment until tested on target
  distributions.
