# TypeText User Guide

**Version {{VERSION}}**

TypeText stores reusable text snippets and types them into other applications. Keep it running in the background, open it with a global shortcut, choose one or more snippets, and then select the destination text field.

> This guide applies to the Windows installer, Windows standard portable, Windows offline portable, and macOS editions. Features that differ by edition are called out where they occur.

## 1. Choose the right edition

### Windows installer

- Best for a normal, single-user Windows installation.
- Stores data in `%LOCALAPPDATA%\TypeText\data`.
- Supports update checks and **Open on Startup**.
- Can create Start menu and optional desktop shortcuts.

### Windows standard portable

- Runs without installation from a local writable folder.
- Stores data in a `data` folder beside `TypeText.exe`.
- Supports update checks and **Open on Startup**.
- Keep the entire TypeText folder together when moving or backing it up.

### Windows offline portable

- Intended for controlled or disconnected environments.
- Stores data only in the adjacent `data` folder and never falls back to AppData.
- Has update checking, external update links, and startup registration removed at build time.
- Refuses UNC paths, mapped network drives, and remote import or export locations.

### macOS

- Distributed as a native application.
- Stores data in `~/Library/Application Support/TypeText/data`.
- Requires Accessibility permission to type into other applications.
- Supports **Open on Startup**.

## 2. First launch

### Windows

1. Install TypeText or extract the portable ZIP to a local writable folder.
2. Start `TypeText.exe`.
3. Leave TypeText running when you want its chooser or favourite shortcuts available.
4. Optionally enable **Settings > Open on Startup**, when that setting is available.

Windows editions use native operating-system features for global shortcuts, returning focus to the target window, and Unicode text insertion. They do not bundle a browser engine or web application runtime.

### macOS

1. Open `TypeText.app`.
2. Open **System Settings > Privacy & Security > Accessibility**.
3. Enable TypeText.
4. If a development build was launched from Terminal, enable the terminal application that launched it.
5. Return to TypeText and optionally enable **Settings > Open on Startup**.

If macOS permission was denied or changed, quit and reopen TypeText after enabling it.

## 3. Learn the window

TypeText has three main areas:

- **Choose** searches snippets, filters by group, and builds the typing queue.
- **Edit** manages groups, snippets, custom tokens, and built-in token references.
- **Settings** controls shortcuts, typing behavior, favourites, appearance, updates, storage, import, and export.

The top-right controls belong to TypeText:

- **Min** hides the app while leaving it available in the background.
- **Max** toggles between normal and maximized window sizes.
- **Exit** quits TypeText and disables its global shortcuts until it is started again.

You can reopen a hidden window with the chooser shortcut or the tray/menu bar icon.

## 4. Configure shortcuts

The default chooser shortcut is:

`Ctrl+Alt+Space`

To change it:

1. Open **Settings > Hotkey**.
2. Capture or enter the new key combination.
3. Click **Save Settings**.
4. Hide TypeText and test the shortcut from another application.

Up to ten favourite snippets can also have direct-insertion shortcuts. Configure these under **Settings > Favourites**. The chooser shortcut and all favourite shortcuts must be unique. Another application or the operating system may already own a shortcut, in which case choose another combination.

## 5. Create groups and snippets

### Create a group

1. Open **Edit > Groups**.
2. Click **Add**.
3. Enter a useful name such as `Email`, `Support`, or `Addresses`.
4. Save the group.

Click an existing group to edit it. Group details appear only after a group is selected or added.

### Create a snippet

1. Open **Edit > Snippets**.
2. Choose the destination group from the header dropdown.
3. Click **Add**.
4. Enter a short, recognizable name.
5. Enter the text to be typed.
6. Click **Save**.

The snippet editor appears only after a snippet is selected or added. Use **Copy** to duplicate a snippet into another group, or **Move** to transfer it. Copying a favourite does not copy its favourite slot; moving one retains its slot.

### Arrange snippets

Each group can use:

- **Custom** order, with **Earlier** and **Later** controls.
- **A-Z** order.
- **Z-A** order.

### Assign a favourite

1. Select a snippet in **Edit > Snippets**.
2. Open **Favourite** beside **Add**.
3. Assign slot 1 through 10, or remove the existing assignment.
4. Confirm if the chosen slot is already occupied.

Favourite snippets show a muted `#1` through `#10` marker in Edit and Choose. Deleting the snippet frees its slot.

## 6. Type a snippet

1. In another application, select or click near the field that will receive the text.
2. Open TypeText with its global shortcut.
3. In **Choose**, search by snippet name or text, or filter by group.
4. Click a snippet to add it to the queue.
5. Add more snippets if needed.
6. Click the destination text field when TypeText asks you to select it.
7. TypeText hides, restores focus, and types the queued content.

Use **Undo Last** to remove the most recently queued item or **Clear** to empty the queue. Clicking an already queued snippet can add it again; this behavior is configurable under **Settings > Selection**.

### Type a favourite directly

After a favourite has both a slot and shortcut:

1. Focus the destination application.
2. Press the favourite shortcut.
3. TypeText types that favourite into the previously focused application without opening the chooser.

## 7. Build reusable content with tokens

Tokens are names inside braces that are replaced immediately before typing. Saved snippet text is not modified, and every snippet in one queue shares the same timestamp.

### Built-in date and time tokens

- `{time.today}` - current time; retained as a legacy DropText alias.
- `{time.now}` - current time, for example `17:42`.
- `{date.today}` - today's date, for example `20/06/2026`.
- `{date.tomorrow}` - tomorrow's date.
- `{date.yesterday}` - yesterday's date.
- `{datetime.now}` - current date and time.
- `{weekday.today}` - current weekday.

Unknown tokens are typed unchanged. To type a supported token literally, double the braces: `{{date.today}}` types `{date.today}`.

### Custom tokens

Custom tokens are useful for values repeated across many snippets, such as a company name, phone number, product version, or sign-off.

1. Open **Edit > Tokens**.
2. Select **Custom Tokens** in the header dropdown.
3. Click **Add**.
4. Enter a name without braces, such as `company.phone`.
5. Enter its value and save.
6. Use `{company.phone}` in any snippet.

Change the custom token once and every snippet uses its new value the next time it is typed. Use **Static Tokens** in the same dropdown to review the built-in date and time tokens. In the snippet editor, the **Tokens** menu inserts a token at the cursor or replaces selected body text.

## 8. Control queue formatting and typing reliability

### Paragraph separators

Under **Settings > Typing**, choose whether each queued snippet starts on a new line and how many empty lines appear between snippets.

- `0` empty lines starts the next snippet on the immediately following line.
- `1` empty line leaves one blank line between snippets.

### Delay before typing

This delay begins after TypeText hides and before typing starts. Increase it if the target application is slow to regain focus or the first characters appear in the wrong place.

### Windows typing delays

Windows provides two additional controls:

- **Character delay** pauses between ordinary characters. The default is 22 ms.
- **Separator delay** pauses after spaces, punctuation, tabs, and newlines. The default is 35 ms.

Fast systems may work at 12-18 ms. If text is incomplete, increase the relevant delay or use **Reset to Default**. Missing spaces or punctuation usually indicates that separator delay should be increased first. macOS uses the general delay before typing and does not expose per-character delays.

## 9. Settings and appearance

Most changes in Settings are staged. If the app shows **Unsaved changes - click Save Settings**, click **Save Settings** before leaving.

Settings include:

- Chooser and favourite shortcuts.
- Open on Startup, except in Windows offline portable.
- Open minimized.
- Delay and Windows typing timing.
- Queue selection and paragraph-separator behavior.
- Favourite slots and their optional shortcuts.
- Light, dark, or system theme.
- Curated accent-color presets or a custom hexadecimal accent.
- Small, default, or large interface sizing.
- Reduced visual effects for interface transitions.
- Update checks, except in Windows offline portable.

## 10. Import, export, storage, and backups

### Import and export

Open **Settings > Snippet Data**:

- **Import** reads DropText INI or CSV snippets.
- **Export** saves TypeText snippets as JSON.
- **Clear All** removes all snippets; export a backup first if they may be needed later.

Review imported content before typing it. A snippet can contain arbitrary text and Enter keys.

### Find your data

Open **Settings > Storage** to see the active data folder, then use **Open Data** to open it. The folder contains:

- `snippets.json`
- `settings.json`
- `tokens.json`

Back up all three files while TypeText is closed. Portable users should back up the whole TypeText folder. The JSON data is readable and is not encrypted.

### Sensitive information

Do not use TypeText to store passwords, recovery codes, API keys, or other secrets. Anyone who can read or modify the data folder can view or change text that TypeText will later type. Use operating-system or full-device encryption where storage needs protection at rest.

## 11. Troubleshooting

### TypeText opens but does not type

- On macOS, confirm Accessibility permission and reopen the app.
- Increase **Delay before typing**.
- Select the destination field after queuing.
- Be aware that secure password fields may reject synthetic typing.

### Text goes to the wrong application

- Queue first, then click the exact destination field.
- Avoid switching to another window while insertion begins.
- Increase **Delay before typing** if focus restoration is slow.

### A shortcut does not work

- Confirm TypeText is still running.
- Save Settings after changing a shortcut.
- Ensure chooser and favourite shortcuts are unique.
- Try a shortcut that is not used by another application.
- Confirm a favourite shortcut has an assigned snippet.

### TypeText disappeared

**Min** hides the app without quitting. Reopen it from the shortcut or tray/menu bar. **Exit** quits it completely.

### Windows misses characters, spaces, or punctuation

- Missing normal characters: increase **Character delay**.
- Missing spaces, punctuation, tabs, or newlines: increase **Separator delay** first.
- Try a separator delay of 50-75 ms, test in the real destination application, and adjust.
- Use **Reset to Default** if needed.

### A custom token appears not to update

The snippet keeps the token name, not a copied value. Confirm that the body contains `{token.name}` and that the matching custom token is saved.

### The wrong group or token list is displayed

- In **Edit > Snippets**, use the header dropdown to select the active group.
- In **Edit > Tokens**, use the header dropdown to switch between Custom and Static Tokens.

### Open on Startup is missing

It is deliberately absent from Windows offline portable. Use the installer or standard portable edition when startup registration is required.

### Offline portable import or export fails

Keep the application, data, import, and export files on local writable storage. Offline portable refuses UNC paths, mapped drives, and remote locations.

## 12. Recommended first setup

1. Start TypeText and grant macOS Accessibility permission if applicable.
2. Choose and save a global shortcut.
3. Enable **Open on Startup** if wanted and available.
4. Add a group and one test snippet.
5. Choose a theme and accent color.
6. Click **Save Settings**.
7. Open a simple text editor, queue the test snippet, select the destination field, and confirm insertion.
8. Adjust typing delays only if the destination application needs them.
9. Export a backup after creating important snippets.

## Edition capability summary

| Capability | Windows installer | Windows portable | Windows offline portable | macOS |
| --- | --- | --- | --- | --- |
| Native desktop app | Yes | Yes | Yes | Yes |
| Per-user data folder | Yes | No | No | Yes |
| Data beside executable | No | Yes | Yes, required | No |
| Update checks | Yes | Yes | No | Yes |
| Open on Startup | Yes | Yes | No | Yes |
| Remote import/export locations | Yes | Yes | No | Yes |
| macOS Accessibility permission | N/A | N/A | N/A | Required |

---

TypeText User Guide - {{VERSION}}
