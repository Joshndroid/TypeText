# Linux Notes

TypeText uses the shared Rust/egui desktop window on Linux. The current Linux
build targets Ubuntu and is packaged as an AppImage portable app plus a DEB
package so the UI and data model can be tested on target distributions.

## AppImage Build

Build the Linux portable AppImage on Linux:

```bash
Scripts/build-linux-portable.sh
```

Output:

```text
dist/TypeText-Linux-<target>.AppImage
```

The AppImage uses the normal Linux per-user data directory because AppImage
contents are mounted read-only at runtime:

```text
$XDG_DATA_HOME/typetext/data or ~/.local/share/typetext/data
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
