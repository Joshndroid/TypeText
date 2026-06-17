# Linux Notes

TypeText uses Electron for the window and global hotkey. Text insertion depends on the desktop session.

## Wayland

Install `wtype` and make sure the compositor allows synthetic input.

## X11

Install `xdotool`.

## Known Limitations

- Wayland support varies by compositor.
- Some desktop environments restrict global hotkeys or synthetic input.
- Linux should be treated as a supported experiment until tested on target distributions.
