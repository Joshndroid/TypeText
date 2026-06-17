# macOS Notes

TypeText uses the Rust desktop window with a Carbon global hotkey, then AppleScript/System Events for typing selected snippets into the previously focused app.

## Permissions

macOS requires Accessibility permission for synthetic keyboard input.

Path:

`System Settings > Privacy & Security > Accessibility`

When running from development, grant permission to the terminal app used to launch TypeText. When running a packaged app, grant permission to TypeText itself.

## Known Limitations

- The first typing attempt may fail until Accessibility permission is granted.
- Some secure fields may block synthetic typing.
- If focus does not return to the original app after the chooser hides, increase the typing delay in settings.
- The app must already be running for the global hotkey to bring it forward.
