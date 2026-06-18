use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Sender};

#[cfg(windows)]
mod windows_platform {
    use super::*;
    use std::mem::size_of;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::thread;
    use windows::Win32::Foundation::{HWND, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, SendInput, UnregisterHotKey, HOT_KEY_MODIFIERS, INPUT, INPUT_0,
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL,
        MOD_SHIFT, MOD_WIN, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetForegroundWindow, GetMessageW, SetForegroundWindow, TranslateMessage,
        MSG, WM_HOTKEY,
    };

    const HOTKEY_ID: i32 = 0x5454;
    static TARGET_WINDOW: AtomicIsize = AtomicIsize::new(0);

    pub fn register_hotkey(hotkey: String, tx: Sender<()>) -> Result<()> {
        let (modifiers, key) =
            parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        thread::spawn(move || unsafe {
            if let Err(error) = RegisterHotKey(None, HOTKEY_ID, modifiers, key) {
                let _ = ready_tx.send(Err(format!("RegisterHotKey failed: {error}")));
                return;
            }
            let _ = ready_tx.send(Ok(()));

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                if msg.message == WM_HOTKEY && msg.wParam == WPARAM(HOTKEY_ID as usize) {
                    remember_target_window();
                    let _ = tx.send(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnregisterHotKey(None, HOTKEY_ID);
        });
        ready_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    pub fn type_text(text: &str) -> Result<()> {
        restore_target_window();
        send_text(text)
    }

    pub fn type_text_current_focus(text: &str) -> Result<()> {
        send_text(text)
    }

    fn send_text(text: &str) -> Result<()> {
        for unit in text.encode_utf16() {
            send_unicode_unit(unit)?;
        }
        Ok(())
    }

    pub fn remember_target_window() {
        let hwnd = unsafe { GetForegroundWindow() };
        TARGET_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    }

    pub fn startup_enabled() -> bool {
        startup_shortcut_path().is_some_and(|path| path.exists())
            || legacy_startup_script_path().is_some_and(|path| path.exists())
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        let shortcut_path = startup_shortcut_path()
            .ok_or_else(|| anyhow!("Could not locate Windows Startup folder"))?;
        let legacy_script_path = legacy_startup_script_path();
        if enabled {
            let exe =
                std::env::current_exe().context("Could not determine current executable path")?;
            write_startup_shortcut(&shortcut_path, &exe)?;
        } else if shortcut_path.exists() {
            std::fs::remove_file(&shortcut_path)
                .with_context(|| format!("Could not remove {}", shortcut_path.display()))?;
        }

        if let Some(legacy_script_path) = legacy_script_path {
            if legacy_script_path.exists() {
                std::fs::remove_file(&legacy_script_path).with_context(|| {
                    format!("Could not remove {}", legacy_script_path.display())
                })?;
            }
        }
        Ok(())
    }

    pub fn tray_status() -> &'static str {
        "TypeText stays running when hidden or closed, and re-opens from its global hotkey. Use Quit to exit."
    }

    fn send_unicode_unit(unit: u16) -> Result<()> {
        let mut inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        let sent = unsafe { SendInput(&mut inputs, size_of::<INPUT>() as i32) };
        if sent == 0 {
            Err(anyhow!("SendInput failed"))
        } else {
            Ok(())
        }
    }

    fn parse_hotkey(value: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
        let mut modifiers = 0;
        let mut key = None;

        for part in value
            .split('+')
            .map(|part| part.trim().to_ascii_lowercase())
        {
            match part.as_str() {
                "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
                "alt" | "option" => modifiers |= MOD_ALT.0,
                "shift" => modifiers |= MOD_SHIFT.0,
                "win" | "windows" => modifiers |= MOD_WIN.0,
                "space" => key = Some(0x20),
                "enter" | "return" => key = Some(0x0D),
                "escape" | "esc" => key = Some(0x1B),
                part if part.len() == 1 => {
                    key = Some(part.as_bytes()[0].to_ascii_uppercase() as u32)
                }
                part if part.starts_with('f') => {
                    let number = part[1..].parse::<u32>().ok()?;
                    if (1..=24).contains(&number) {
                        key = Some(0x70 + number - 1);
                    }
                }
                _ => {}
            }
        }

        key.map(|key| (HOT_KEY_MODIFIERS(modifiers), key))
    }

    pub fn open_folder(path: &Path) -> Result<()> {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn open_url(url: &str) -> Result<()> {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn fetch_text(url: &str) -> Result<String> {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$ProgressPreference='SilentlyContinue'; (Invoke-WebRequest -UseBasicParsing -Headers @{'User-Agent'='TypeText'} -Uri $args[0]).Content",
            ])
            .arg(url)
            .output()
            .context("Could not run PowerShell update check")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Update request failed. {}", stderr.trim()))
        }
    }

    pub fn open_droptext_file_dialog() -> Result<Option<PathBuf>> {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Import DropText.ini'
$dialog.Filter = 'DropText INI (*.ini)|*.ini|All files (*.*)|*.*'
$dialog.CheckFileExists = $true
$dialog.CheckPathExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.FileName)
}
"#;
        let output = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .context("Could not open Windows file dialog")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Windows file dialog failed. {}", stderr.trim()));
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    pub fn open_snippets_export_dialog() -> Result<Option<PathBuf>> {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.SaveFileDialog
$dialog.Title = 'Export TypeText snippets'
$dialog.Filter = 'TypeText snippets (*.json)|*.json|All files (*.*)|*.*'
$dialog.FileName = 'snippets.json'
$dialog.DefaultExt = 'json'
$dialog.AddExtension = $true
$dialog.OverwritePrompt = $true
$dialog.CheckPathExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.FileName)
}
"#;
        let output = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .context("Could not open Windows save dialog")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Windows save dialog failed. {}", stderr.trim()));
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    fn restore_target_window() {
        let raw = TARGET_WINDOW.load(Ordering::Relaxed);
        if raw != 0 {
            let _ = unsafe { SetForegroundWindow(HWND(raw as *mut std::ffi::c_void)) };
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }

    fn write_startup_shortcut(shortcut_path: &Path, exe_path: &Path) -> Result<()> {
        let script = r#"
$shortcutPath = $args[0]
$targetPath = $args[1]
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $targetPath
$shortcut.WorkingDirectory = Split-Path -Parent $targetPath
$shortcut.IconLocation = "$targetPath,0"
$shortcut.Save()
"#;
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .arg(shortcut_path)
            .arg(exe_path)
            .output()
            .context("Could not create Windows startup shortcut")?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "Windows startup shortcut creation failed. {}",
                stderr.trim()
            ))
        }
    }

    fn startup_shortcut_path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("Microsoft\\Windows\\Start Menu\\Programs\\Startup\\TypeText.lnk"),
        )
    }

    fn legacy_startup_script_path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("Microsoft\\Windows\\Start Menu\\Programs\\Startup\\TypeText.cmd"),
        )
    }
}

#[cfg(target_os = "macos")]
mod macos_platform {
    use super::*;
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::ptr;
    use std::sync::{Mutex, OnceLock};
    use std::thread;

    type OSStatus = i32;
    type UInt32 = u32;
    type EventTargetRef = *mut c_void;
    type EventHandlerCallRef = *mut c_void;
    type EventRef = *mut c_void;
    type EventHandlerRef = *mut c_void;
    type EventHotKeyRef = *mut c_void;
    type EventHandlerUPP = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

    #[repr(C)]
    struct EventTypeSpec {
        event_class: UInt32,
        event_kind: UInt32,
    }

    #[repr(C)]
    struct EventHotKeyID {
        signature: UInt32,
        id: UInt32,
    }

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> EventTargetRef;
        fn InstallEventHandler(
            target: EventTargetRef,
            handler: EventHandlerUPP,
            num_types: UInt32,
            list: *const EventTypeSpec,
            user_data: *mut c_void,
            handler_ref: *mut EventHandlerRef,
        ) -> OSStatus;
        fn RegisterEventHotKey(
            hot_key_code: UInt32,
            hot_key_modifiers: UInt32,
            hot_key_id: EventHotKeyID,
            target: EventTargetRef,
            options: UInt32,
            hot_key_ref: *mut EventHotKeyRef,
        ) -> OSStatus;
        fn RunApplicationEventLoop();
    }

    const HOTKEY_SIGNATURE: UInt32 = u32::from_be_bytes(*b"TyTx");
    const HOTKEY_ID: UInt32 = 1;
    const K_EVENT_CLASS_KEYBOARD: UInt32 = u32::from_be_bytes(*b"keyb");
    const K_EVENT_HOT_KEY_PRESSED: UInt32 = 5;

    const CMD_KEY: UInt32 = 1 << 8;
    const SHIFT_KEY: UInt32 = 1 << 9;
    const OPTION_KEY: UInt32 = 1 << 11;
    const CONTROL_KEY: UInt32 = 1 << 12;

    const NO_ERR: OSStatus = 0;
    static TARGET_APPLICATION: OnceLock<Mutex<Option<String>>> = OnceLock::new();

    pub fn register_hotkey(hotkey: String, tx: Sender<()>) -> Result<()> {
        let (modifiers, key_code) =
            parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;

        let (ready_tx, ready_rx) = mpsc::channel();
        thread::spawn(move || unsafe {
            let target = GetApplicationEventTarget();
            let event_type = EventTypeSpec {
                event_class: K_EVENT_CLASS_KEYBOARD,
                event_kind: K_EVENT_HOT_KEY_PRESSED,
            };
            let tx = Box::into_raw(Box::new(tx));
            let handler_status = InstallEventHandler(
                target,
                hotkey_handler,
                1,
                &event_type,
                tx.cast::<c_void>(),
                ptr::null_mut(),
            );
            if handler_status != NO_ERR {
                let _ = Box::from_raw(tx);
                let _ = ready_tx.send(Err(format!(
                    "InstallEventHandler failed with status {handler_status}"
                )));
                return;
            }

            let hotkey_id = EventHotKeyID {
                signature: HOTKEY_SIGNATURE,
                id: HOTKEY_ID,
            };
            let mut hotkey_ref = ptr::null_mut();
            let register_status =
                RegisterEventHotKey(key_code, modifiers, hotkey_id, target, 0, &mut hotkey_ref);
            if register_status != NO_ERR {
                let _ = Box::from_raw(tx);
                let _ = ready_tx.send(Err(format!(
                    "RegisterEventHotKey failed with status {register_status}"
                )));
                return;
            }
            let _ = ready_tx.send(Ok(()));

            RunApplicationEventLoop();
        });
        ready_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    pub fn type_text(text: &str) -> Result<()> {
        restore_target_application()?;
        type_text_current_focus(text)
    }

    pub fn type_text_current_focus(text: &str) -> Result<()> {
        let mut args = Vec::new();
        for line in apple_script_for_text(text) {
            args.push("-e".to_string());
            args.push(line);
        }

        let output = Command::new("osascript")
            .args(args)
            .output()
            .context("Could not run osascript")?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "macOS typing failed. Grant Accessibility permission to your terminal or TypeText. {}",
                stderr.trim()
            ))
        }
    }

    pub fn open_folder(path: &Path) -> Result<()> {
        Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn open_url(url: &str) -> Result<()> {
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn fetch_text(url: &str) -> Result<String> {
        let output = Command::new("curl")
            .args(["-fsSL", "-H", "User-Agent: TypeText", url])
            .output()
            .context("Could not run curl update check")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Update request failed. {}", stderr.trim()))
        }
    }

    pub fn open_droptext_file_dialog() -> Result<Option<PathBuf>> {
        let script = r#"
try
    set chosenFile to choose file with prompt "Import DropText.ini" of type {"ini", "txt"}
    return POSIX path of chosenFile
on error number -128
    return ""
end try
"#;
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .context("Could not open macOS file dialog")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("macOS file dialog failed. {}", stderr.trim()));
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    pub fn open_snippets_export_dialog() -> Result<Option<PathBuf>> {
        let script = r#"
try
    set chosenFile to choose file name with prompt "Export TypeText snippets" default name "snippets.json"
    return POSIX path of chosenFile
on error number -128
    return ""
end try
"#;
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .context("Could not open macOS save dialog")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("macOS save dialog failed. {}", stderr.trim()));
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    pub fn startup_enabled() -> bool {
        launch_agent_path().is_some_and(|path| path.exists())
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        let launch_agent_path =
            launch_agent_path().ok_or_else(|| anyhow!("Could not locate LaunchAgents folder"))?;

        if enabled {
            let plist = launch_agent_plist()?;
            let launch_agents_dir = launch_agent_path
                .parent()
                .ok_or_else(|| anyhow!("Could not locate LaunchAgents folder"))?;
            std::fs::create_dir_all(launch_agents_dir)
                .with_context(|| format!("Could not create {}", launch_agents_dir.display()))?;
            std::fs::write(&launch_agent_path, plist)
                .with_context(|| format!("Could not write {}", launch_agent_path.display()))?;
        } else if launch_agent_path.exists() {
            std::fs::remove_file(&launch_agent_path)
                .with_context(|| format!("Could not remove {}", launch_agent_path.display()))?;
        }

        Ok(())
    }

    pub fn tray_status() -> &'static str {
        "TypeText stays running when hidden or closed, and re-opens from its global hotkey. Use Quit to exit."
    }

    extern "C" fn hotkey_handler(
        _next_handler: EventHandlerCallRef,
        _event: EventRef,
        user_data: *mut c_void,
    ) -> OSStatus {
        remember_target_application();
        let tx = unsafe { &*(user_data.cast::<Sender<()>>()) };
        let _ = tx.send(());
        NO_ERR
    }

    fn parse_hotkey(value: &str) -> Option<(UInt32, UInt32)> {
        let mut modifiers = 0;
        let mut key_code = None;

        for part in value
            .split('+')
            .map(|part| part.trim().to_ascii_lowercase())
        {
            match part.as_str() {
                "ctrl" | "control" => modifiers |= CONTROL_KEY,
                "alt" | "option" => modifiers |= OPTION_KEY,
                "shift" => modifiers |= SHIFT_KEY,
                "win" | "windows" | "cmd" | "command" => modifiers |= CMD_KEY,
                "space" => key_code = Some(49),
                "enter" | "return" => key_code = Some(36),
                "escape" | "esc" => key_code = Some(53),
                "tab" => key_code = Some(48),
                part if part.len() == 1 => key_code = key_code_for_character(part.as_bytes()[0]),
                part if part.starts_with('f') => {
                    let number = part[1..].parse::<u32>().ok()?;
                    key_code = function_key_code(number);
                }
                _ => {}
            }
        }

        key_code.map(|key_code| (modifiers, key_code))
    }

    fn key_code_for_character(character: u8) -> Option<UInt32> {
        match character.to_ascii_uppercase() {
            b'A' => Some(0),
            b'S' => Some(1),
            b'D' => Some(2),
            b'F' => Some(3),
            b'H' => Some(4),
            b'G' => Some(5),
            b'Z' => Some(6),
            b'X' => Some(7),
            b'C' => Some(8),
            b'V' => Some(9),
            b'B' => Some(11),
            b'Q' => Some(12),
            b'W' => Some(13),
            b'E' => Some(14),
            b'R' => Some(15),
            b'Y' => Some(16),
            b'T' => Some(17),
            b'O' => Some(31),
            b'U' => Some(32),
            b'I' => Some(34),
            b'P' => Some(35),
            b'L' => Some(37),
            b'J' => Some(38),
            b'K' => Some(40),
            b'N' => Some(45),
            b'M' => Some(46),
            b'1' => Some(18),
            b'2' => Some(19),
            b'3' => Some(20),
            b'4' => Some(21),
            b'6' => Some(22),
            b'5' => Some(23),
            b'=' => Some(24),
            b'9' => Some(25),
            b'7' => Some(26),
            b'-' => Some(27),
            b'8' => Some(28),
            b'0' => Some(29),
            _ => None,
        }
    }

    fn function_key_code(number: u32) -> Option<UInt32> {
        match number {
            1 => Some(122),
            2 => Some(120),
            3 => Some(99),
            4 => Some(118),
            5 => Some(96),
            6 => Some(97),
            7 => Some(98),
            8 => Some(100),
            9 => Some(101),
            10 => Some(109),
            11 => Some(103),
            12 => Some(111),
            13 => Some(105),
            14 => Some(107),
            15 => Some(113),
            16 => Some(106),
            17 => Some(64),
            18 => Some(79),
            19 => Some(80),
            20 => Some(90),
            _ => None,
        }
    }

    fn remember_target_application() {
        let Ok(output) = Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
            .output()
        else {
            return;
        };

        if !output.status.success() {
            return;
        }

        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() && name != "TypeText" {
            let target = TARGET_APPLICATION.get_or_init(|| Mutex::new(None));
            if let Ok(mut target) = target.lock() {
                *target = Some(name);
            }
        }
    }

    fn restore_target_application() -> Result<()> {
        let Some(name) = TARGET_APPLICATION
            .get()
            .and_then(|target| target.lock().ok().and_then(|target| target.clone()))
        else {
            return Ok(());
        };

        let script = format!(
            "tell application \"{}\" to activate",
            escape_apple_script(&name)
        );
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .context("Could not restore target application")?;

        if output.status.success() {
            std::thread::sleep(std::time::Duration::from_millis(80));
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "Could not restore target application before typing. {}",
                stderr.trim()
            ))
        }
    }

    fn apple_script_for_text(text: &str) -> Vec<String> {
        let mut script = vec!["tell application \"System Events\"".to_string()];
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines = normalized.split('\n').peekable();

        while let Some(line) = lines.next() {
            if !line.is_empty() {
                script.push(format!("keystroke \"{}\"", escape_apple_script(line)));
            }
            if lines.peek().is_some() {
                script.push("key code 36".to_string());
            }
        }

        script.push("end tell".to_string());
        script
    }

    fn escape_apple_script(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    fn launch_agent_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("LaunchAgents")
                .join("com.typetext.desktop.plist"),
        )
    }

    fn launch_agent_plist() -> Result<String> {
        let program_arguments = startup_program_arguments()?
            .into_iter()
            .map(|argument| format!("    <string>{}</string>", escape_xml(&argument)))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.typetext.desktop</string>
  <key>ProgramArguments</key>
  <array>
{program_arguments}
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
        ))
    }

    fn startup_program_arguments() -> Result<Vec<String>> {
        let exe = std::env::current_exe().context("Could not determine current executable path")?;
        if let Some(app_bundle) = current_app_bundle(&exe) {
            Ok(vec![
                "/usr/bin/open".to_string(),
                app_bundle.display().to_string(),
            ])
        } else {
            Ok(vec![exe.display().to_string()])
        }
    }

    fn current_app_bundle(exe: &Path) -> Option<PathBuf> {
        let contents_dir = exe.parent()?.parent()?;
        if contents_dir.file_name()? != "Contents" {
            return None;
        }

        let app_bundle = contents_dir.parent()?;
        if app_bundle.extension()? == "app" {
            Some(app_bundle.to_path_buf())
        } else {
            None
        }
    }

    fn escape_xml(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
}

#[cfg(target_os = "linux")]
mod linux_platform {
    use super::*;
    use std::collections::HashMap;
    use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
    use std::ptr;
    use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

    type Display = c_void;
    type Window = c_ulong;
    type KeySym = c_ulong;

    #[repr(C)]
    struct XKeyEvent {
        type_: c_int,
        serial: c_ulong,
        send_event: c_int,
        display: *mut Display,
        window: Window,
        root: Window,
        subwindow: Window,
        time: c_ulong,
        x: c_int,
        y: c_int,
        x_root: c_int,
        y_root: c_int,
        state: c_uint,
        keycode: c_uint,
        same_screen: c_int,
    }

    #[repr(C)]
    struct XErrorEvent {
        type_: c_int,
        display: *mut Display,
        resourceid: c_ulong,
        serial: c_ulong,
        error_code: u8,
        request_code: u8,
        minor_code: u8,
    }

    #[repr(C)]
    union XEvent {
        type_: c_int,
        xkey: std::mem::ManuallyDrop<XKeyEvent>,
        pad: [c_long; 24],
    }

    #[link(name = "X11")]
    extern "C" {
        fn XOpenDisplay(display_name: *const c_char) -> *mut Display;
        fn XCloseDisplay(display: *mut Display) -> c_int;
        fn XDefaultRootWindow(display: *mut Display) -> Window;
        fn XKeysymToKeycode(display: *mut Display, keysym: KeySym) -> c_uint;
        fn XGrabKey(
            display: *mut Display,
            keycode: c_int,
            modifiers: c_uint,
            grab_window: Window,
            owner_events: c_int,
            pointer_mode: c_int,
            keyboard_mode: c_int,
        ) -> c_int;
        fn XNextEvent(display: *mut Display, event_return: *mut XEvent) -> c_int;
        fn XGetInputFocus(
            display: *mut Display,
            focus_return: *mut Window,
            revert_to_return: *mut c_int,
        ) -> c_int;
        fn XSetErrorHandler(
            handler: Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>,
        ) -> Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>;
        fn XSync(display: *mut Display, discard: c_int) -> c_int;
    }

    const KEY_PRESS: c_int = 2;
    const GRAB_MODE_ASYNC: c_int = 1;

    const SHIFT_MASK: c_uint = 1 << 0;
    const LOCK_MASK: c_uint = 1 << 1;
    const CONTROL_MASK: c_uint = 1 << 2;
    const MOD1_MASK: c_uint = 1 << 3;
    const MOD2_MASK: c_uint = 1 << 4;
    const MOD4_MASK: c_uint = 1 << 6;

    const XK_BACK_SPACE: KeySym = 0xff08;
    const XK_TAB: KeySym = 0xff09;
    const XK_RETURN: KeySym = 0xff0d;
    const XK_ESCAPE: KeySym = 0xff1b;
    const XK_DELETE: KeySym = 0xffff;
    const XK_F1: KeySym = 0xffbe;
    const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
    const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
    const PORTAL_GLOBAL_SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
    const PORTAL_REMOTE_DESKTOP_INTERFACE: &str = "org.freedesktop.portal.RemoteDesktop";
    const PORTAL_REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";
    const SHORTCUT_ID_SHOW: &str = "show-typetext";
    const REMOTE_DESKTOP_DEVICE_KEYBOARD: u32 = 1;
    const KEY_RELEASED: u32 = 0;
    const KEY_PRESSED: u32 = 1;
    const XK_CARRIAGE_RETURN: i32 = 0xff0d;
    const XK_TAB_I32: i32 = 0xff09;

    static TARGET_WINDOW: AtomicU64 = AtomicU64::new(0);
    static LAST_X_ERROR: AtomicI32 = AtomicI32::new(0);
    static REMOTE_DESKTOP_SESSION: OnceLock<Mutex<Option<RemoteDesktopSession>>> = OnceLock::new();

    struct RemoteDesktopSession {
        connection: Connection,
        session_handle: OwnedObjectPath,
    }

    pub fn register_hotkey(hotkey: String, tx: Sender<()>) -> Result<()> {
        if is_wayland_session() {
            return register_portal_hotkey(hotkey, tx);
        }

        let (modifiers, keysym) =
            parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;

        let (ready_tx, ready_rx) = mpsc::channel();
        thread::spawn(move || unsafe {
            let display = XOpenDisplay(ptr::null());
            if display.is_null() {
                let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
                let detail = if session.eq_ignore_ascii_case("wayland") {
                    " Ubuntu Wayland sessions do not expose X11 global key grabs; choose an Ubuntu on Xorg session to use TypeText's global hotkey."
                } else {
                    " Ensure DISPLAY is set and an X11 session is running."
                };
                let _ = ready_tx.send(Err(format!("Could not open X11 display.{detail}")));
                return;
            }

            let root = XDefaultRootWindow(display);
            let keycode = XKeysymToKeycode(display, keysym);
            if keycode == 0 {
                let _ = XCloseDisplay(display);
                let _ = ready_tx.send(Err(format!("Unsupported hotkey key: {hotkey}")));
                return;
            }

            let previous_handler = XSetErrorHandler(Some(x_error_handler));
            LAST_X_ERROR.store(0, Ordering::SeqCst);
            for variant in modifier_variants(modifiers) {
                XGrabKey(
                    display,
                    keycode as c_int,
                    variant,
                    root,
                    0,
                    GRAB_MODE_ASYNC,
                    GRAB_MODE_ASYNC,
                );
            }
            XSync(display, 0);
            XSetErrorHandler(previous_handler);

            let error_code = LAST_X_ERROR.load(Ordering::SeqCst);
            if error_code != 0 {
                let _ = XCloseDisplay(display);
                let _ = ready_tx.send(Err(format!(
                    "X11 could not register {hotkey}; another app or the desktop environment may already own it. X error code {error_code}."
                )));
                return;
            }

            let _ = ready_tx.send(Ok(()));

            loop {
                let mut event = XEvent { pad: [0; 24] };
                if XNextEvent(display, &mut event) != 0 {
                    continue;
                }
                if event.type_ == KEY_PRESS {
                    remember_target_window(display);
                    let _ = tx.send(());
                }
            }
        });

        ready_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    pub fn type_text(text: &str) -> Result<()> {
        if is_wayland_session() {
            return type_text_wayland(text);
        }
        type_text_x11(text)
    }

    pub fn type_text_current_focus(text: &str) -> Result<()> {
        type_text(text)
    }

    pub fn open_folder(path: &Path) -> Result<()> {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn open_url(url: &str) -> Result<()> {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn fetch_text(url: &str) -> Result<String> {
        let output = Command::new("curl")
            .args(["-fsSL", "-H", "User-Agent: TypeText", url])
            .output()
            .context("Could not run curl update check")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Update request failed. {}", stderr.trim()))
        }
    }

    pub fn open_droptext_file_dialog() -> Result<Option<std::path::PathBuf>> {
        Err(anyhow!(
            "Native DropText file picker is only implemented on macOS and Windows."
        ))
    }

    pub fn open_snippets_export_dialog() -> Result<Option<std::path::PathBuf>> {
        Err(anyhow!(
            "Native snippets export picker is only implemented on macOS and Windows."
        ))
    }

    pub fn startup_enabled() -> bool {
        false
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        if enabled {
            Err(anyhow!(
                "Open on Startup only applies to the macOS and Windows builds."
            ))
        } else {
            Ok(())
        }
    }

    pub fn tray_status() -> &'static str {
        "TypeText stays running when hidden or closed, and re-opens from its global hotkey. Ubuntu uses the desktop portal on Wayland or X11 key grabs on Xorg."
    }

    fn register_portal_hotkey(hotkey: String, tx: Sender<()>) -> Result<()> {
        let preferred_trigger = portal_trigger_for_hotkey(&hotkey)
            .ok_or_else(|| anyhow!("Invalid Wayland portal hotkey: {hotkey}"))?;

        let (ready_tx, ready_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = run_portal_hotkey_loop(&hotkey, &preferred_trigger, tx, ready_tx.clone());
            if let Err(error) = result {
                let _ = ready_tx.send(Err(error.to_string()));
            }
        });

        ready_rx
            .recv()
            .unwrap_or_else(|_| Err("Wayland portal hotkey registration stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    fn run_portal_hotkey_loop(
        hotkey: &str,
        preferred_trigger: &str,
        tx: Sender<()>,
        ready_tx: Sender<std::result::Result<(), String>>,
    ) -> Result<()> {
        let connection = Connection::session()
            .context("Could not connect to the D-Bus session bus for Wayland hotkeys")?;
        let proxy = Proxy::new(
            &connection,
            PORTAL_DESTINATION,
            PORTAL_PATH,
            PORTAL_GLOBAL_SHORTCUTS_INTERFACE,
        )
        .context("Could not connect to the desktop global-shortcuts portal")?;

        let version = proxy.get_property::<u32>("version").unwrap_or(0);
        if version == 0 {
            return Err(anyhow!(
                "This Wayland desktop does not expose the GlobalShortcuts portal."
            ));
        }

        let session_handle = create_portal_session(&proxy)?;
        bind_portal_shortcut(&proxy, &session_handle, hotkey, preferred_trigger)?;
        let _ = ready_tx.send(Ok(()));

        let mut signals = proxy
            .receive_signal("Activated")
            .context("Could not subscribe to Wayland global shortcut activation")?;
        for signal in &mut signals {
            let Ok((activated_session, shortcut_id, _timestamp, _options)) = signal
                .body()
                .deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()
            else {
                continue;
            };
            if activated_session == session_handle && shortcut_id == SHORTCUT_ID_SHOW {
                let _ = tx.send(());
            }
        }

        Ok(())
    }

    fn type_text_wayland(text: &str) -> Result<()> {
        let session_store = REMOTE_DESKTOP_SESSION.get_or_init(|| Mutex::new(None));
        let mut session_guard = session_store
            .lock()
            .map_err(|_| anyhow!("Wayland remote desktop session lock was poisoned"))?;

        if session_guard.is_none() {
            *session_guard = Some(start_remote_desktop_keyboard_session()?);
        }

        let session = session_guard
            .as_ref()
            .ok_or_else(|| anyhow!("Wayland remote desktop keyboard session was unavailable"))?;
        if let Err(error) = send_text_via_remote_desktop(session, text) {
            *session_guard = None;
            return Err(error);
        }

        Ok(())
    }

    fn start_remote_desktop_keyboard_session() -> Result<RemoteDesktopSession> {
        let connection = Connection::session()
            .context("Could not connect to the D-Bus session bus for Wayland typing")?;
        let proxy = Proxy::new(
            &connection,
            PORTAL_DESTINATION,
            PORTAL_PATH,
            PORTAL_REMOTE_DESKTOP_INTERFACE,
        )
        .context("Could not connect to the desktop remote-control portal")?;

        let version = proxy.get_property::<u32>("version").unwrap_or(0);
        if version == 0 {
            return Err(anyhow!(
                "This Wayland desktop does not expose the RemoteDesktop portal needed for typing."
            ));
        }

        let available_devices = proxy
            .get_property::<u32>("AvailableDeviceTypes")
            .unwrap_or(0);
        if available_devices & REMOTE_DESKTOP_DEVICE_KEYBOARD == 0 {
            return Err(anyhow!(
                "This Wayland desktop does not allow portal keyboard input."
            ));
        }

        let session_handle = create_remote_desktop_session(&proxy)?;
        select_remote_desktop_keyboard(&proxy, &session_handle)?;
        start_remote_desktop_session(&proxy, &session_handle)?;

        Ok(RemoteDesktopSession {
            connection,
            session_handle,
        })
    }

    fn create_remote_desktop_session(proxy: &Proxy<'_>) -> Result<OwnedObjectPath> {
        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(portal_token("remote_create")));
        options.insert(
            "session_handle_token",
            Value::from(portal_token("remote_session")),
        );

        let request_handle: OwnedObjectPath = proxy
            .call("CreateSession", &(options,))
            .context("Could not create Wayland typing portal session")?;
        let results = wait_for_portal_response(proxy.connection(), &request_handle)?;
        let session_handle = results
            .get("session_handle")
            .ok_or_else(|| anyhow!("Wayland typing portal did not return a session"))?;
        let session_handle = String::try_from(session_handle.clone())
            .context("Wayland typing portal returned an invalid session handle")?;
        OwnedObjectPath::try_from(session_handle)
            .context("Wayland typing portal returned a malformed session handle")
    }

    fn select_remote_desktop_keyboard(
        proxy: &Proxy<'_>,
        session_handle: &OwnedObjectPath,
    ) -> Result<()> {
        let session_path = ObjectPath::try_from(session_handle.as_str())
            .context("Wayland typing portal session handle was malformed")?;
        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(portal_token("remote_select")));
        options.insert("types", Value::from(REMOTE_DESKTOP_DEVICE_KEYBOARD));
        options.insert("persist_mode", Value::from(1u32));

        let request_handle: OwnedObjectPath = proxy
            .call("SelectDevices", &(session_path, options))
            .context("Could not request Wayland keyboard input permission")?;
        wait_for_portal_response(proxy.connection(), &request_handle).map(|_| ())
    }

    fn start_remote_desktop_session(
        proxy: &Proxy<'_>,
        session_handle: &OwnedObjectPath,
    ) -> Result<()> {
        let session_path = ObjectPath::try_from(session_handle.as_str())
            .context("Wayland typing portal session handle was malformed")?;
        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(portal_token("remote_start")));

        let request_handle: OwnedObjectPath = proxy
            .call("Start", &(session_path, "", options))
            .context("Could not start Wayland keyboard input session")?;
        let results = wait_for_portal_response(proxy.connection(), &request_handle)?;
        let devices = results
            .get("devices")
            .and_then(|value| u32::try_from(value.clone()).ok())
            .unwrap_or(0);
        if devices & REMOTE_DESKTOP_DEVICE_KEYBOARD == 0 {
            return Err(anyhow!(
                "Wayland keyboard input permission was not granted."
            ));
        }

        Ok(())
    }

    fn send_text_via_remote_desktop(session: &RemoteDesktopSession, text: &str) -> Result<()> {
        let proxy = Proxy::new(
            &session.connection,
            PORTAL_DESTINATION,
            PORTAL_PATH,
            PORTAL_REMOTE_DESKTOP_INTERFACE,
        )
        .context("Could not connect to the active Wayland typing portal session")?;
        let session_path = ObjectPath::try_from(session.session_handle.as_str())
            .context("Wayland typing portal session handle was malformed")?;

        for keysym in text.chars().map(char_to_keysym) {
            notify_keyboard_keysym(&proxy, &session_path, keysym, KEY_PRESSED)?;
            notify_keyboard_keysym(&proxy, &session_path, keysym, KEY_RELEASED)?;
        }

        Ok(())
    }

    fn notify_keyboard_keysym(
        proxy: &Proxy<'_>,
        session_path: &ObjectPath<'_>,
        keysym: i32,
        state: u32,
    ) -> Result<()> {
        let options = HashMap::<&str, Value<'_>>::new();
        proxy
            .call::<_, _, ()>(
                "NotifyKeyboardKeysym",
                &(session_path, options, keysym, state),
            )
            .context("Wayland keyboard input failed")
    }

    fn char_to_keysym(character: char) -> i32 {
        match character {
            '\n' => XK_CARRIAGE_RETURN,
            '\r' => XK_CARRIAGE_RETURN,
            '\t' => XK_TAB_I32,
            character => {
                let codepoint = character as u32;
                if (0x20..=0x7e).contains(&codepoint) || (0xa0..=0xff).contains(&codepoint) {
                    codepoint as i32
                } else {
                    (0x01000000 | codepoint) as i32
                }
            }
        }
    }

    fn type_text_x11(text: &str) -> Result<()> {
        let target_window = TARGET_WINDOW.load(Ordering::Relaxed);
        let mut command = Command::new("xdotool");
        if target_window != 0 {
            command.args(["windowactivate", "--sync", &target_window.to_string()]);
        }
        command.args(["type", "--clearmodifiers", "--delay", "0", "--"]);
        command.arg(text);

        let output = command
            .output()
            .context("Could not run xdotool. Install xdotool to type snippets on X11/Xorg.")?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("X11 typing failed. {}", stderr.trim()))
        }
    }

    fn create_portal_session(proxy: &Proxy<'_>) -> Result<OwnedObjectPath> {
        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(portal_token("create")));
        options.insert("session_handle_token", Value::from(portal_token("session")));

        let request_handle: OwnedObjectPath = proxy
            .call("CreateSession", &(options,))
            .context("Could not create Wayland global-shortcuts portal session")?;
        let results = wait_for_portal_response(proxy.connection(), &request_handle)?;
        let session_handle = results
            .get("session_handle")
            .ok_or_else(|| anyhow!("Wayland global-shortcuts portal did not return a session"))?;
        let session_handle = String::try_from(session_handle.clone())
            .context("Wayland global-shortcuts portal returned an invalid session handle")?;
        OwnedObjectPath::try_from(session_handle)
            .context("Wayland global-shortcuts portal returned a malformed session handle")
    }

    fn bind_portal_shortcut(
        proxy: &Proxy<'_>,
        session_handle: &OwnedObjectPath,
        hotkey: &str,
        preferred_trigger: &str,
    ) -> Result<()> {
        let session_path = ObjectPath::try_from(session_handle.as_str())
            .context("Wayland global-shortcuts portal session handle was malformed")?;
        let mut shortcut_options = HashMap::new();
        shortcut_options.insert("description", Value::from("Show TypeText"));
        shortcut_options.insert(
            "preferred_trigger",
            Value::from(preferred_trigger.to_string()),
        );
        let shortcuts = vec![(SHORTCUT_ID_SHOW, shortcut_options)];

        let mut options = HashMap::new();
        options.insert("handle_token", Value::from(portal_token("bind")));

        let request_handle: OwnedObjectPath = proxy
            .call("BindShortcuts", &(session_path, shortcuts, "", options))
            .with_context(|| format!("Could not ask Wayland to bind {hotkey}"))?;
        let results = wait_for_portal_response(proxy.connection(), &request_handle)?;
        let shortcuts = results
            .get("shortcuts")
            .ok_or_else(|| anyhow!("Wayland global-shortcuts portal did not bind {hotkey}"))?;
        let shortcuts =
            Vec::<(String, HashMap<String, OwnedValue>)>::try_from(shortcuts.clone())
                .context("Wayland global-shortcuts portal returned an invalid shortcut list")?;
        if shortcuts
            .iter()
            .any(|(shortcut_id, _)| shortcut_id == SHORTCUT_ID_SHOW)
        {
            Ok(())
        } else {
            Err(anyhow!(
                "Wayland did not bind {hotkey}. The shortcut may have been denied or already reserved."
            ))
        }
    }

    fn wait_for_portal_response(
        connection: &Connection,
        request_handle: &OwnedObjectPath,
    ) -> Result<HashMap<String, OwnedValue>> {
        let request_proxy = Proxy::new(
            connection,
            PORTAL_DESTINATION,
            request_handle.as_str(),
            PORTAL_REQUEST_INTERFACE,
        )
        .context("Could not listen for Wayland portal response")?;
        let mut responses = request_proxy
            .receive_signal("Response")
            .context("Could not subscribe to Wayland portal response")?;
        let Some(response) = responses.next() else {
            return Err(anyhow!("Wayland portal response stream closed"));
        };
        let (response_code, results): (u32, HashMap<String, OwnedValue>) = response
            .body()
            .deserialize()
            .context("Could not parse Wayland portal response")?;
        match response_code {
            0 => Ok(results),
            1 => Err(anyhow!("Wayland global shortcut setup was cancelled")),
            2 => Err(anyhow!("Wayland global shortcut setup was denied")),
            code => Err(anyhow!(
                "Wayland global shortcut setup failed with portal response {code}"
            )),
        }
    }

    fn portal_trigger_for_hotkey(value: &str) -> Option<String> {
        let mut modifiers = Vec::new();
        let mut key = None;

        for part in value.split('+').map(|part| part.trim()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.push("CTRL".to_string()),
                "alt" | "option" => modifiers.push("ALT".to_string()),
                "shift" => modifiers.push("SHIFT".to_string()),
                "win" | "windows" | "cmd" | "command" | "super" | "meta" => {
                    modifiers.push("LOGO".to_string())
                }
                "space" => key = Some("space".to_string()),
                "enter" | "return" => key = Some("Return".to_string()),
                "escape" | "esc" => key = Some("Escape".to_string()),
                "tab" => key = Some("Tab".to_string()),
                "backspace" => key = Some("BackSpace".to_string()),
                "delete" | "del" => key = Some("Delete".to_string()),
                part if part.len() == 1 => key = Some(part.to_ascii_lowercase()),
                part if part.starts_with('f') => {
                    let number = part[1..].parse::<u32>().ok()?;
                    if (1..=35).contains(&number) {
                        key = Some(format!("F{number}"));
                    }
                }
                _ => {}
            }
        }

        let key = key?;
        modifiers.push(key);
        Some(modifiers.join("+"))
    }

    fn portal_token(prefix: &str) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        format!("typetext_{prefix}_{millis}")
    }

    fn is_wayland_session() -> bool {
        std::env::var("XDG_SESSION_TYPE")
            .map(|session| session.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
    }

    fn parse_hotkey(value: &str) -> Option<(c_uint, KeySym)> {
        let mut modifiers = 0;
        let mut keysym = None;

        for part in value
            .split('+')
            .map(|part| part.trim().to_ascii_lowercase())
        {
            match part.as_str() {
                "ctrl" | "control" => modifiers |= CONTROL_MASK,
                "alt" | "option" => modifiers |= MOD1_MASK,
                "shift" => modifiers |= SHIFT_MASK,
                "win" | "windows" | "cmd" | "command" | "super" | "meta" => modifiers |= MOD4_MASK,
                "space" => keysym = Some(' ' as KeySym),
                "enter" | "return" => keysym = Some(XK_RETURN),
                "escape" | "esc" => keysym = Some(XK_ESCAPE),
                "tab" => keysym = Some(XK_TAB),
                "backspace" => keysym = Some(XK_BACK_SPACE),
                "delete" | "del" => keysym = Some(XK_DELETE),
                part if part.len() == 1 => {
                    let byte = part.as_bytes()[0];
                    keysym = Some(byte.to_ascii_uppercase() as KeySym);
                }
                part if part.starts_with('f') => {
                    let number = part[1..].parse::<u32>().ok()?;
                    if (1..=35).contains(&number) {
                        keysym = Some(XK_F1 + KeySym::from(number - 1));
                    }
                }
                _ => {}
            }
        }

        keysym.map(|keysym| (modifiers, keysym))
    }

    fn modifier_variants(modifiers: c_uint) -> [c_uint; 4] {
        [
            modifiers,
            modifiers | LOCK_MASK,
            modifiers | MOD2_MASK,
            modifiers | LOCK_MASK | MOD2_MASK,
        ]
    }

    fn remember_target_window(display: *mut Display) {
        let mut focus = 0;
        let mut revert_to = 0;
        unsafe {
            XGetInputFocus(display, &mut focus, &mut revert_to);
        }
        if focus != 0 {
            TARGET_WINDOW.store(focus as u64, Ordering::Relaxed);
        }
    }

    unsafe extern "C" fn x_error_handler(_display: *mut Display, error: *mut XErrorEvent) -> c_int {
        if !error.is_null() {
            LAST_X_ERROR.store((*error).error_code as i32, Ordering::SeqCst);
        }
        0
    }
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
mod fallback_platform {
    use super::*;

    pub fn register_hotkey(_hotkey: String, _tx: Sender<()>) -> Result<()> {
        Err(anyhow!(
            "Global hotkey is not implemented on this platform yet."
        ))
    }

    pub fn type_text(_text: &str) -> Result<()> {
        Err(anyhow!("Typing is not implemented on this platform yet."))
    }

    pub fn type_text_current_focus(_text: &str) -> Result<()> {
        Err(anyhow!("Typing is not implemented on this platform yet."))
    }

    pub fn open_folder(path: &Path) -> Result<()> {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn open_url(url: &str) -> Result<()> {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn fetch_text(url: &str) -> Result<String> {
        let output = Command::new("curl")
            .args(["-fsSL", "-H", "User-Agent: TypeText", url])
            .output()
            .context("Could not run curl update check")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!("Update request failed. {}", stderr.trim()))
        }
    }

    pub fn open_droptext_file_dialog() -> Result<Option<std::path::PathBuf>> {
        Err(anyhow!(
            "Native DropText file picker is only implemented on macOS and Windows."
        ))
    }

    pub fn open_snippets_export_dialog() -> Result<Option<std::path::PathBuf>> {
        Err(anyhow!(
            "Native snippets export picker is only implemented on macOS and Windows."
        ))
    }

    pub fn startup_enabled() -> bool {
        false
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        if enabled {
            Err(anyhow!(
                "Open on Startup only applies to the macOS and Windows builds."
            ))
        } else {
            Ok(())
        }
    }

    pub fn tray_status() -> &'static str {
        "Tray integration is targeted at the Windows build."
    }
}

#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
pub use fallback_platform::{
    fetch_text, open_droptext_file_dialog, open_folder, open_snippets_export_dialog, open_url,
    register_hotkey, set_startup_enabled, startup_enabled, tray_status, type_text,
    type_text_current_focus,
};
#[cfg(target_os = "linux")]
pub use linux_platform::{
    fetch_text, open_droptext_file_dialog, open_folder, open_snippets_export_dialog, open_url,
    register_hotkey, set_startup_enabled, startup_enabled, tray_status, type_text,
    type_text_current_focus,
};
#[cfg(target_os = "macos")]
pub use macos_platform::{
    fetch_text, open_droptext_file_dialog, open_folder, open_snippets_export_dialog, open_url,
    register_hotkey, set_startup_enabled, startup_enabled, tray_status, type_text,
    type_text_current_focus,
};
#[cfg(windows)]
pub use windows_platform::{
    fetch_text, open_droptext_file_dialog, open_folder, open_snippets_export_dialog, open_url,
    register_hotkey, set_startup_enabled, startup_enabled, tray_status, type_text,
    type_text_current_focus,
};
