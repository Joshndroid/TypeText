#[cfg(any(windows, target_os = "macos", not(feature = "offline-portable")))]
use anyhow::Context;
use anyhow::{Result, anyhow};
use std::path::Path;
#[cfg(not(windows))]
use std::process::Command;
#[cfg(any(windows, target_os = "macos"))]
use std::sync::mpsc;
use std::sync::mpsc::Sender;

use crate::TrayCommand;

#[cfg(any(windows, target_os = "macos"))]
mod tray_integration {
    use super::*;
    use eframe::egui;
    use std::thread;
    use std::time::Duration;
    use tray_icon::{
        Icon, TrayIcon, TrayIconBuilder,
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    };

    pub struct TrayHandle {
        _tray_icon: TrayIcon,
    }

    pub fn install_tray_icon(
        tx: Sender<TrayCommand>,
        repaint_ctx: egui::Context,
        icon_rgba: Option<(Vec<u8>, u32, u32)>,
    ) -> Result<TrayHandle> {
        let open_item = MenuItem::new("Open", true, None);
        let settings_item = MenuItem::new("Settings", true, None);
        let exit_item = MenuItem::new("Exit", true, None);
        let open_id = open_item.id().clone();
        let settings_id = settings_item.id().clone();
        let exit_id = exit_item.id().clone();

        let menu = Menu::new();
        let separator = PredefinedMenuItem::separator();
        menu.append_items(&[&open_item, &settings_item, &separator, &exit_item])
            .context("Could not build tray menu")?;

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("TypeText is running in the background");
        if let Some((rgba, width, height)) = icon_rgba {
            match Icon::from_rgba(rgba, width, height) {
                Ok(icon) => builder = builder.with_icon(icon),
                Err(error) => eprintln!("Could not load tray icon: {error}"),
            }
        }

        let tray_icon = builder.build().context("Could not create tray icon")?;

        thread::spawn(move || {
            let menu_rx = MenuEvent::receiver();
            loop {
                if let Ok(event) = menu_rx.try_recv() {
                    let command = if event.id == open_id {
                        Some(TrayCommand::Open)
                    } else if event.id == settings_id {
                        Some(TrayCommand::Settings)
                    } else if event.id == exit_id {
                        Some(TrayCommand::Exit)
                    } else {
                        None
                    };

                    if let Some(command) = command {
                        let should_exit = command == TrayCommand::Exit;
                        let _ = tx.send(command);
                        repaint_ctx.request_repaint();
                        if should_exit {
                            break;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        });

        Ok(TrayHandle {
            _tray_icon: tray_icon,
        })
    }
}

#[cfg(windows)]
mod windows_platform {
    use super::*;
    use std::mem::size_of;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::mpsc::Receiver;
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND, WPARAM,
    };
    #[cfg(not(feature = "offline-portable"))]
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    #[cfg(not(feature = "offline-portable"))]
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS,
        REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW,
    };
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey, SendInput,
        UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT,
        VK_SPACE, VK_TAB,
    };
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, MSG, PM_REMOVE,
        PeekMessageW, SW_SHOWNORMAL, SetForegroundWindow, TranslateMessage, WM_HOTKEY,
    };
    use windows::core::{PCWSTR, w};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DRIVE_REMOTE_TYPE: u32 = 4;
    const HOTKEY_ID: i32 = 0x5454;
    #[cfg(not(feature = "offline-portable"))]
    const STARTUP_RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    #[cfg(not(feature = "offline-portable"))]
    const STARTUP_RUN_VALUE: &str = "TypeText";
    static APP_MUTEX_HANDLE: AtomicIsize = AtomicIsize::new(0);
    static TARGET_WINDOW: AtomicIsize = AtomicIsize::new(0);
    static TARGET_PROCESS_ID: AtomicU32 = AtomicU32::new(0);
    static HOTKEY_MANAGER: OnceLock<Sender<HotkeyCommand>> = OnceLock::new();

    pub fn install_app_mutex() -> Result<()> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("TypeTextAppMutex"))
                .context("Could not create TypeText app mutex")?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return Err(anyhow!("another TypeText instance is already running"));
            }

            // Win32 HANDLE values are not closed on Rust drop. Retain the raw value
            // for clarity and let Windows release it when the process exits.
            APP_MUTEX_HANDLE.store(handle.0 as isize, Ordering::Relaxed);
        }
        Ok(())
    }

    enum HotkeyCommand {
        Reregister {
            hotkey: String,
            reply_tx: Sender<Result<(), String>>,
        },
    }

    struct ActiveHotkey {
        hotkey: String,
        modifiers: HOT_KEY_MODIFIERS,
        key: u32,
        tx: Sender<()>,
        repaint_ctx: eframe::egui::Context,
    }

    pub fn register_hotkey(
        hotkey: String,
        tx: Sender<()>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<()> {
        let (modifiers, key) =
            parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        thread::spawn(move || {
            let active = ActiveHotkey {
                hotkey,
                modifiers,
                key,
                tx,
                repaint_ctx,
            };
            run_hotkey_manager(active, command_rx, ready_tx);
        });
        let _ = HOTKEY_MANAGER.set(command_tx);
        ready_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    pub fn reregister_hotkey(hotkey: String, _tx: Sender<()>) -> Result<()> {
        parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;
        let manager = HOTKEY_MANAGER
            .get()
            .ok_or_else(|| anyhow!("Hotkey registration thread is not running"))?;
        let (reply_tx, reply_rx) = mpsc::channel();
        manager
            .send(HotkeyCommand::Reregister { hotkey, reply_tx })
            .map_err(|_| anyhow!("Hotkey registration thread stopped"))?;
        reply_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    fn run_hotkey_manager(
        mut active: ActiveHotkey,
        command_rx: Receiver<HotkeyCommand>,
        ready_tx: Sender<Result<(), String>>,
    ) {
        unsafe {
            if let Err(error) = RegisterHotKey(None, HOTKEY_ID, active.modifiers, active.key) {
                let _ = ready_tx.send(Err(format!("RegisterHotKey failed: {error}")));
                return;
            }
        }
        let _ = ready_tx.send(Ok(()));

        loop {
            while let Ok(command) = command_rx.try_recv() {
                match command {
                    HotkeyCommand::Reregister { hotkey, reply_tx } => {
                        let result = replace_hotkey(&mut active, hotkey);
                        let _ = reply_tx.send(result);
                    }
                }
            }

            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                    if msg.message == WM_HOTKEY && msg.wParam == WPARAM(HOTKEY_ID as usize) {
                        remember_target_window();
                        let _ = active.tx.send(());
                        active.repaint_ctx.request_repaint();
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            thread::sleep(Duration::from_millis(20));
        }
    }

    fn replace_hotkey(active: &mut ActiveHotkey, hotkey: String) -> Result<(), String> {
        let (modifiers, key) =
            parse_hotkey(&hotkey).ok_or_else(|| format!("Invalid hotkey: {hotkey}"))?;
        unsafe {
            let _ = UnregisterHotKey(None, HOTKEY_ID);
            if let Err(error) = RegisterHotKey(None, HOTKEY_ID, modifiers, key) {
                let restore_result = RegisterHotKey(None, HOTKEY_ID, active.modifiers, active.key);
                if let Err(restore_error) = restore_result {
                    return Err(format!(
                        "RegisterHotKey failed: {error}; could not restore {}: {restore_error}",
                        active.hotkey
                    ));
                }
                return Err(format!("RegisterHotKey failed: {error}"));
            }
        }
        active.hotkey = hotkey;
        active.modifiers = modifiers;
        active.key = key;
        Ok(())
    }

    pub fn type_text(text: &str, character_delay_ms: u64, separator_delay_ms: u64) -> Result<()> {
        let target = restore_target_window()?;
        send_text(text, character_delay_ms, separator_delay_ms, target)
    }

    pub fn type_text_current_focus(
        text: &str,
        character_delay_ms: u64,
        separator_delay_ms: u64,
    ) -> Result<()> {
        let target = unsafe { GetForegroundWindow() };
        if target.0.is_null() {
            return Err(anyhow!("No target window is focused; no text was typed"));
        }
        send_text(text, character_delay_ms, separator_delay_ms, target)
    }

    fn send_text(
        text: &str,
        character_delay_ms: u64,
        separator_delay_ms: u64,
        expected_target: HWND,
    ) -> Result<()> {
        ensure_target_is_foreground(expected_target)?;
        release_modifier_keys()?;
        thread::sleep(Duration::from_millis(20));
        let character_interval = Duration::from_millis(character_delay_ms);
        let separator_interval = Duration::from_millis(separator_delay_ms);
        for character in text.chars() {
            ensure_target_is_foreground(expected_target)?;
            if character == '\r' {
                continue;
            }

            if character == '\n' {
                send_virtual_key(VK_RETURN)?;
                thread::sleep(separator_interval);
                continue;
            }

            if character == ' ' {
                send_virtual_key(VK_SPACE)?;
                thread::sleep(separator_interval);
                continue;
            }

            if character == '\t' {
                send_virtual_key(VK_TAB)?;
                thread::sleep(separator_interval);
                continue;
            }

            for unit in character.encode_utf16(&mut [0; 2]) {
                send_unicode_unit(*unit)?;
                thread::sleep(unicode_input_interval(
                    *unit,
                    character_interval,
                    separator_interval,
                ));
            }
        }
        Ok(())
    }

    pub fn remember_target_window() {
        let hwnd = unsafe { GetForegroundWindow() };
        TARGET_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
        let mut process_id = 0u32;
        if !hwnd.0.is_null() {
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        }
        TARGET_PROCESS_ID.store(process_id, Ordering::Relaxed);
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn startup_enabled() -> bool {
        startup_registry_value().is_some()
    }

    #[cfg(feature = "offline-portable")]
    pub fn startup_enabled() -> bool {
        false
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        if enabled {
            let exe =
                std::env::current_exe().context("Could not determine current executable path")?;
            let expected = startup_command(&exe);
            write_startup_registry_entry(&expected)?;
            let actual = startup_registry_value()
                .ok_or_else(|| anyhow!("Windows startup entry was not created"))?;
            if actual != expected {
                return Err(anyhow!(
                    "Windows startup entry verification failed. Expected {expected}, found {actual}"
                ));
            }
        } else {
            remove_startup_registry_entry()?;
            if startup_registry_value().is_some() {
                return Err(anyhow!("Windows startup entry was not removed"));
            }
        }
        Ok(())
    }

    #[cfg(feature = "offline-portable")]
    pub fn set_startup_enabled(_enabled: bool) -> Result<()> {
        Err(anyhow!("Startup registration is disabled in this build"))
    }

    pub fn tray_status() -> &'static str {
        "TypeText stays running when hidden or closed, and re-opens from its tray icon or global hotkey. Use Exit in the tray menu or Quit here to close it."
    }

    pub fn install_reopen_handler(_tx: Sender<TrayCommand>, _repaint_ctx: eframe::egui::Context) {}

    fn send_unicode_unit(unit: u16) -> Result<()> {
        let inputs = [
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

        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        if sent == 0 {
            Err(anyhow!("SendInput failed"))
        } else {
            Ok(())
        }
    }

    fn unicode_input_interval(
        unit: u16,
        character_interval: Duration,
        separator_interval: Duration,
    ) -> Duration {
        match char::from_u32(unit as u32) {
            Some(' ' | '\t' | '\n' | '\r' | '.' | ',' | ';' | ':' | '!' | '?') => {
                separator_interval
            }
            _ => character_interval,
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
                "tab" => key = Some(0x09),
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
        let path = wide_null(&path.to_string_lossy());
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            Err(anyhow!("Windows could not open the data folder"))
        } else {
            Ok(())
        }
    }

    pub fn storage_security_warning(path: &Path) -> Option<String> {
        use std::path::{Component, Prefix};

        if path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(prefix)
                    if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))
            )
        }) {
            return Some(
                "This portable data folder is on a network path. File reads and writes may cross the network even though update and web features are absent."
                    .to_string(),
            );
        }

        let drive_root = path.components().find_map(|component| match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    Some(format!("{}:\\", letter as char))
                }
                _ => None,
            },
            _ => None,
        });
        let root = drive_root.map(|root| wide_null(&root))?;
        if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } == DRIVE_REMOTE_TYPE {
            Some(
                "This portable data folder is on a mapped network drive. File reads and writes may cross the network even though update and web features are absent."
                    .to_string(),
            )
        } else {
            None
        }
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn open_url(url: &str) -> Result<()> {
        let mut command = hidden_command("rundll32");
        command
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn fetch_text(url: &str) -> Result<String> {
        let output = hidden_command("powershell")
            .env("TYPETEXT_UPDATE_URL", url)
            .args([
                "-NoProfile",
                "-Command",
                "$ProgressPreference='SilentlyContinue'; $uri=[Environment]::GetEnvironmentVariable('TYPETEXT_UPDATE_URL'); (Invoke-WebRequest -UseBasicParsing -TimeoutSec 30 -Headers @{'User-Agent'='TypeText'} -Uri $uri).Content",
            ])
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
$dialog.Title = 'Import DropText snippets'
$dialog.Filter = 'DropText files (*.ini;*.csv)|*.ini;*.csv|DropText INI (*.ini)|*.ini|DropText CSV (*.csv)|*.csv|All files (*.*)|*.*'
$dialog.CheckFileExists = $true
$dialog.CheckPathExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.FileName)
}
"#;
        let output = hidden_command("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .context("Could not open Windows file dialog")?;

        selected_path_from_dialog_output(output, "Windows file dialog failed")
    }

    pub fn open_snippets_export_dialog(initial_dir: &Path) -> Result<Option<PathBuf>> {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.SaveFileDialog
$dialog.Title = 'Export TypeText snippets'
$dialog.Filter = 'TypeText snippets (*.json)|*.json|All files (*.*)|*.*'
$dialog.FileName = 'snippets.json'
$dialog.InitialDirectory = [Environment]::GetEnvironmentVariable('TYPETEXT_EXPORT_DIR')
$dialog.DefaultExt = 'json'
$dialog.AddExtension = $true
$dialog.OverwritePrompt = $true
$dialog.CheckPathExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Out.Write($dialog.FileName)
}
"#;
        let output = hidden_command("powershell")
            .env("TYPETEXT_EXPORT_DIR", initial_dir)
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .context("Could not open Windows save dialog")?;

        selected_path_from_dialog_output(output, "Windows save dialog failed")
    }

    fn selected_path_from_dialog_output(
        output: std::process::Output,
        failure_message: &str,
    ) -> Result<Option<PathBuf>> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("{failure_message}. {}", stderr.trim()));
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    fn release_modifier_keys() -> Result<()> {
        for key in [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN] {
            send_key_up(key)?;
        }
        Ok(())
    }

    fn send_key_up(key: VIRTUAL_KEY) -> Result<()> {
        send_virtual_key_with_flags(key, KEYEVENTF_KEYUP)
    }

    fn send_virtual_key(key: VIRTUAL_KEY) -> Result<()> {
        send_virtual_key_with_flags(key, Default::default())?;
        thread::sleep(Duration::from_millis(8));
        send_virtual_key_with_flags(key, KEYEVENTF_KEYUP)
    }

    fn send_virtual_key_with_flags(
        key: VIRTUAL_KEY,
        flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
    ) -> Result<()> {
        let inputs = [INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }];

        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        if sent == 0 {
            Err(anyhow!("SendInput failed"))
        } else {
            Ok(())
        }
    }

    fn restore_target_window() -> Result<HWND> {
        let raw = TARGET_WINDOW.load(Ordering::Relaxed);
        if raw == 0 {
            return Err(anyhow!("The original target window is no longer available"));
        }
        let target = HWND(raw as *mut std::ffi::c_void);
        if !unsafe { IsWindow(Some(target)) }.as_bool() {
            return Err(anyhow!(
                "The original target window has closed; no text was typed"
            ));
        }

        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(target, Some(&mut process_id)) };
        if process_id == 0 || process_id != TARGET_PROCESS_ID.load(Ordering::Relaxed) {
            return Err(anyhow!(
                "The original target window changed; no text was typed"
            ));
        }
        if !unsafe { SetForegroundWindow(target) }.as_bool() {
            return Err(anyhow!(
                "Windows refused to focus the target window; no text was typed"
            ));
        }

        for _ in 0..10 {
            if unsafe { GetForegroundWindow() } == target {
                return Ok(target);
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(anyhow!(
            "The target window did not receive focus; no text was typed"
        ))
    }

    fn ensure_target_is_foreground(expected: HWND) -> Result<()> {
        if unsafe { GetForegroundWindow() } == expected {
            Ok(())
        } else {
            Err(anyhow!(
                "Typing stopped because focus moved to another window"
            ))
        }
    }

    #[cfg(not(feature = "offline-portable"))]
    fn startup_command(exe_path: &Path) -> String {
        format!("\"{}\" --startup", exe_path.display())
    }

    #[cfg(not(feature = "offline-portable"))]
    fn startup_registry_value() -> Option<String> {
        unsafe {
            let key = open_startup_registry_key(KEY_READ).ok()?;
            let value_name = wide_null(STARTUP_RUN_VALUE);
            let mut value_type = REG_SZ;
            let mut byte_len = 0u32;
            let query_size = RegQueryValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                None,
                Some(&mut value_type),
                None,
                Some(&mut byte_len),
            );
            if query_size != ERROR_SUCCESS || value_type != REG_SZ || byte_len == 0 {
                let _ = RegCloseKey(key);
                return None;
            }

            let mut buffer = vec![0u16; (byte_len as usize).div_ceil(2)];
            let query_value = RegQueryValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                None,
                Some(&mut value_type),
                Some(buffer.as_mut_ptr().cast::<u8>()),
                Some(&mut byte_len),
            );
            let _ = RegCloseKey(key);
            if query_value != ERROR_SUCCESS || value_type != REG_SZ {
                return None;
            }

            let end = buffer
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(buffer.len());
            Some(String::from_utf16_lossy(&buffer[..end]))
        }
    }

    #[cfg(not(feature = "offline-portable"))]
    fn write_startup_registry_entry(value: &str) -> Result<()> {
        unsafe {
            let key = create_startup_registry_key()?;
            let value_name = wide_null(STARTUP_RUN_VALUE);
            let value_data = wide_null(value);
            let byte_len = (value_data.len() * size_of::<u16>()) as u32;
            let status = RegSetValueExW(
                key,
                PCWSTR(value_name.as_ptr()),
                None,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    value_data.as_ptr().cast::<u8>(),
                    byte_len as usize,
                )),
            );
            let _ = RegCloseKey(key);
            if status != ERROR_SUCCESS {
                return Err(anyhow!("Windows startup entry creation failed: {status:?}"));
            }
        }
        Ok(())
    }

    #[cfg(not(feature = "offline-portable"))]
    fn remove_startup_registry_entry() -> Result<()> {
        unsafe {
            let key = match open_startup_registry_key(KEY_SET_VALUE) {
                Ok(key) => key,
                Err(_) => return Ok(()),
            };
            let value_name = wide_null(STARTUP_RUN_VALUE);
            let status = RegDeleteValueW(key, PCWSTR(value_name.as_ptr()));
            let _ = RegCloseKey(key);
            if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                return Err(anyhow!("Windows startup entry removal failed: {status:?}"));
            }
        }
        Ok(())
    }

    #[cfg(not(feature = "offline-portable"))]
    unsafe fn create_startup_registry_key() -> Result<HKEY> {
        let subkey = wide_null(STARTUP_RUN_SUBKEY);
        let mut key = HKEY::default();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(key)
        } else {
            Err(anyhow!(
                "Could not open Windows startup registry key: {status:?}"
            ))
        }
    }

    #[cfg(not(feature = "offline-portable"))]
    unsafe fn open_startup_registry_key(access: REG_SAM_FLAGS) -> Result<HKEY> {
        let subkey = wide_null(STARTUP_RUN_SUBKEY);
        let mut key = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                access,
                &mut key,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(key)
        } else {
            Err(anyhow!(
                "Could not open Windows startup registry key: {status:?}"
            ))
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn hidden_command(program: &str) -> std::process::Command {
        let mut command = std::process::Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
}

#[cfg(target_os = "macos")]
mod macos_platform {
    use super::*;
    use eframe::egui;
    use std::ffi::CStr;
    use std::ffi::c_char;
    use std::ffi::c_void;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::path::PathBuf;
    use std::process::Stdio;
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
    type ObjcId = *mut c_void;
    type ObjcSel = *mut c_void;
    type ObjcClass = *mut c_void;
    type ObjcImp = extern "C" fn(ObjcId, ObjcSel, ObjcId, bool) -> bool;

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
    unsafe extern "C" {
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
        fn UnregisterEventHotKey(hot_key_ref: EventHotKeyRef) -> OSStatus;
        fn RunApplicationEventLoop();
    }

    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> ObjcClass;
        fn sel_registerName(name: *const c_char) -> ObjcSel;
        #[link_name = "objc_msgSend"]
        fn objc_msg_send(receiver: ObjcId, selector: ObjcSel, ...) -> ObjcId;
        fn class_addMethod(
            class: ObjcClass,
            name: ObjcSel,
            imp: ObjcImp,
            types: *const c_char,
        ) -> bool;
    }

    #[link(name = "ServiceManagement", kind = "framework")]
    unsafe extern "C" {}

    const HOTKEY_SIGNATURE: UInt32 = u32::from_be_bytes(*b"TyTx");
    const HOTKEY_ID: UInt32 = 1;
    const K_EVENT_CLASS_KEYBOARD: UInt32 = u32::from_be_bytes(*b"keyb");
    const K_EVENT_HOT_KEY_PRESSED: UInt32 = 5;

    const CMD_KEY: UInt32 = 1 << 8;
    const SHIFT_KEY: UInt32 = 1 << 9;
    const OPTION_KEY: UInt32 = 1 << 11;
    const CONTROL_KEY: UInt32 = 1 << 12;

    const NO_ERR: OSStatus = 0;
    const SM_APP_SERVICE_STATUS_NOT_REGISTERED: isize = 0;
    const SM_APP_SERVICE_STATUS_ENABLED: isize = 1;
    const SM_APP_SERVICE_STATUS_REQUIRES_APPROVAL: isize = 2;
    static TARGET_APPLICATION: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    static REOPEN_TX: OnceLock<Sender<TrayCommand>> = OnceLock::new();
    static REOPEN_REPAINT_CTX: OnceLock<egui::Context> = OnceLock::new();
    static HOTKEY_STATE: OnceLock<Mutex<Option<ActiveHotkey>>> = OnceLock::new();

    #[derive(Clone)]
    struct ActiveHotkey {
        hotkey: String,
        modifiers: UInt32,
        key_code: UInt32,
        hotkey_ref: usize,
    }

    pub fn register_hotkey(
        hotkey: String,
        tx: Sender<()>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<()> {
        let (modifiers, key_code) =
            parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;

        let (ready_tx, ready_rx) = mpsc::channel();
        thread::spawn(move || unsafe {
            let target = GetApplicationEventTarget();
            let event_type = EventTypeSpec {
                event_class: K_EVENT_CLASS_KEYBOARD,
                event_kind: K_EVENT_HOT_KEY_PRESSED,
            };
            let tx = Box::into_raw(Box::new((tx, repaint_ctx)));
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

            match register_carbon_hotkey(key_code, modifiers, target) {
                Ok(hotkey_ref) => {
                    let state = HOTKEY_STATE.get_or_init(|| Mutex::new(None));
                    *state.lock().expect("hotkey state poisoned") = Some(ActiveHotkey {
                        hotkey,
                        modifiers,
                        key_code,
                        hotkey_ref: hotkey_ref as usize,
                    });
                }
                Err(register_status) => {
                    let _ = Box::from_raw(tx);
                    let _ = ready_tx.send(Err(format!(
                        "RegisterEventHotKey failed with status {register_status}"
                    )));
                    return;
                }
            }
            let _ = ready_tx.send(Ok(()));

            RunApplicationEventLoop();
        });
        ready_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    pub fn install_app_mutex() -> Result<()> {
        static APP_LOCK: OnceLock<File> = OnceLock::new();
        const LOCK_EXCLUSIVE_NONBLOCKING: i32 = 2 | 4;

        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }

        let lock_path = macos_app_lock_path()?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("Could not open app lock {}", lock_path.display()))?;
        if unsafe { flock(lock.as_raw_fd(), LOCK_EXCLUSIVE_NONBLOCKING) } != 0 {
            return Err(anyhow!("another TypeText instance is already running"));
        }
        APP_LOCK
            .set(lock)
            .map_err(|_| anyhow!("TypeText app lock was already initialized"))?;
        Ok(())
    }

    fn macos_app_lock_path() -> Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("Could not locate the user home directory"))?;
        let lock_dir = home
            .join("Library")
            .join("Application Support")
            .join("TypeText");
        fs::create_dir_all(&lock_dir)
            .with_context(|| format!("Could not create app lock folder {}", lock_dir.display()))?;
        Ok(lock_dir.join("typetext.lock"))
    }

    pub fn reregister_hotkey(hotkey: String, _tx: Sender<()>) -> Result<()> {
        let (modifiers, key_code) =
            parse_hotkey(&hotkey).ok_or_else(|| anyhow!("Invalid hotkey: {hotkey}"))?;
        let state = HOTKEY_STATE
            .get()
            .ok_or_else(|| anyhow!("Hotkey registration thread is not running"))?;
        let mut state = state.lock().expect("hotkey state poisoned");
        let active = state
            .clone()
            .ok_or_else(|| anyhow!("Hotkey is not currently registered"))?;

        unsafe {
            let unregister_status = UnregisterEventHotKey(active.hotkey_ref as EventHotKeyRef);
            if unregister_status != NO_ERR {
                return Err(anyhow!(
                    "UnregisterEventHotKey failed with status {unregister_status}"
                ));
            }

            let target = GetApplicationEventTarget();
            match register_carbon_hotkey(key_code, modifiers, target) {
                Ok(hotkey_ref) => {
                    *state = Some(ActiveHotkey {
                        hotkey,
                        modifiers,
                        key_code,
                        hotkey_ref: hotkey_ref as usize,
                    });
                    Ok(())
                }
                Err(register_status) => {
                    match register_carbon_hotkey(active.key_code, active.modifiers, target) {
                        Ok(restored_ref) => {
                            *state = Some(ActiveHotkey {
                                hotkey_ref: restored_ref as usize,
                                ..active
                            });
                            Err(anyhow!(
                                "RegisterEventHotKey failed with status {register_status}"
                            ))
                        }
                        Err(restore_status) => Err(anyhow!(
                            "RegisterEventHotKey failed with status {register_status}; could not restore {}: {restore_status}",
                            active.hotkey
                        )),
                    }
                }
            }
        }
    }

    unsafe fn register_carbon_hotkey(
        key_code: UInt32,
        modifiers: UInt32,
        target: EventTargetRef,
    ) -> std::result::Result<EventHotKeyRef, OSStatus> {
        let hotkey_id = EventHotKeyID {
            signature: HOTKEY_SIGNATURE,
            id: HOTKEY_ID,
        };
        let mut hotkey_ref = ptr::null_mut();
        let register_status = unsafe {
            RegisterEventHotKey(key_code, modifiers, hotkey_id, target, 0, &mut hotkey_ref)
        };
        if register_status != NO_ERR {
            Err(register_status)
        } else {
            Ok(hotkey_ref)
        }
    }

    pub fn install_reopen_handler(tx: Sender<TrayCommand>, repaint_ctx: egui::Context) {
        let _ = REOPEN_TX.set(tx);
        let _ = REOPEN_REPAINT_CTX.set(repaint_ctx);

        unsafe {
            let class = objc_getClass(c"WinitApplicationDelegate".as_ptr());
            if class.is_null() {
                return;
            }

            let selector =
                sel_registerName(c"applicationShouldHandleReopen:hasVisibleWindows:".as_ptr());
            if selector.is_null() {
                return;
            }

            let _ = class_addMethod(
                class,
                selector,
                application_should_handle_reopen,
                c"B@:@B".as_ptr(),
            );
        }
    }

    pub fn type_text(text: &str, _character_delay_ms: u64, _separator_delay_ms: u64) -> Result<()> {
        restore_target_application()?;
        type_text_current_focus(text, _character_delay_ms, _separator_delay_ms)
    }

    pub fn type_text_current_focus(
        text: &str,
        _character_delay_ms: u64,
        _separator_delay_ms: u64,
    ) -> Result<()> {
        let script = apple_script_for_text(text).join("\n");
        let mut child = typing_command()
            .spawn()
            .context("Could not run osascript")?;

        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow!("Could not open osascript stdin"))?;
            stdin
                .write_all(script.as_bytes())
                .context("Could not send text to osascript")?;
        }

        let output = child
            .wait_with_output()
            .context("Could not wait for osascript")?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow!(
                "macOS typing failed because TypeText needs Accessibility permission to send keyboard input. Open System Settings > Privacy & Security > Accessibility, then enable the terminal app that launched TypeText during development, or enable TypeText when running the packaged app. {}",
                stderr.trim()
            ))
        }
    }

    fn typing_command() -> Command {
        let mut command = Command::new("osascript");
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    pub fn open_folder(path: &Path) -> Result<()> {
        Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn storage_security_warning(_path: &Path) -> Option<String> {
        None
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn open_url(url: &str) -> Result<()> {
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn fetch_text(url: &str) -> Result<String> {
        let output = Command::new("curl")
            .args([
                "-fsSL",
                "--connect-timeout",
                "10",
                "--max-time",
                "30",
                "-H",
                "User-Agent: TypeText",
                url,
            ])
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
    set chosenFile to choose file with prompt "Import DropText snippets" of type {"ini", "csv", "txt"}
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

    pub fn open_snippets_export_dialog(initial_dir: &Path) -> Result<Option<PathBuf>> {
        let initial_dir = initial_dir.display().to_string();
        let script = r#"
try
    set initialFolder to POSIX file (system attribute "TYPETEXT_EXPORT_DIR")
    set chosenFile to choose file name with prompt "Export TypeText snippets" default name "snippets.json" default location initialFolder
    return POSIX path of chosenFile
on error number -128
    return ""
end try
"#;
        let output = Command::new("osascript")
            .env("TYPETEXT_EXPORT_DIR", initial_dir)
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
        match main_app_service_status() {
            Some(SM_APP_SERVICE_STATUS_ENABLED) => true,
            Some(
                SM_APP_SERVICE_STATUS_NOT_REGISTERED | SM_APP_SERVICE_STATUS_REQUIRES_APPROVAL,
            )
            | None => false,
            Some(_) => false,
        }
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        if enabled {
            register_main_app_service()?;
        } else {
            unregister_main_app_service()?;
        }

        Ok(())
    }

    pub fn tray_status() -> &'static str {
        "TypeText stays running when hidden or closed, and re-opens from its tray icon or global hotkey. Use Exit in the tray menu or Quit here to close it."
    }

    extern "C" fn hotkey_handler(
        _next_handler: EventHandlerCallRef,
        _event: EventRef,
        user_data: *mut c_void,
    ) -> OSStatus {
        remember_target_application();
        let (tx, repaint_ctx) =
            unsafe { &*(user_data.cast::<(Sender<()>, eframe::egui::Context)>()) };
        let _ = tx.send(());
        repaint_ctx.request_repaint();
        NO_ERR
    }

    extern "C" fn application_should_handle_reopen(
        _self: ObjcId,
        _cmd: ObjcSel,
        _application: ObjcId,
        has_visible_windows: bool,
    ) -> bool {
        if !has_visible_windows {
            if let Some(tx) = REOPEN_TX.get() {
                let _ = tx.send(TrayCommand::Open);
            }
            if let Some(ctx) = REOPEN_REPAINT_CTX.get() {
                ctx.request_repaint();
            }
        }

        true
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

    fn main_app_service_status() -> Option<isize> {
        unsafe {
            let service = main_app_service()?;
            let selector = sel_registerName(c"status".as_ptr());
            if selector.is_null() {
                return None;
            }
            Some(objc_msg_send(service, selector) as isize)
        }
    }

    fn register_main_app_service() -> Result<()> {
        unsafe {
            let service = main_app_service().ok_or_else(|| {
                anyhow!(
                    "Could not locate macOS login item service. {}",
                    macos_startup_manual_instructions()
                )
            })?;
            let selector = sel_registerName(c"registerAndReturnError:".as_ptr());
            if selector.is_null() {
                return Err(anyhow!(
                    "Could not locate macOS login item register selector. {}",
                    macos_startup_manual_instructions()
                ));
            }

            let mut error = ptr::null_mut();
            if objc_msg_send(service, selector, &mut error).is_null() {
                return Err(anyhow!(
                    "Could not register TypeText to open at login. {}",
                    macos_startup_error_details(ns_error_description(error))
                ));
            }

            match main_app_service_status() {
                Some(SM_APP_SERVICE_STATUS_ENABLED) => Ok(()),
                Some(SM_APP_SERVICE_STATUS_REQUIRES_APPROVAL) => Err(anyhow!(
                    "TypeText is registered as a macOS login item, but macOS still requires approval. {}",
                    macos_startup_manual_instructions()
                )),
                Some(SM_APP_SERVICE_STATUS_NOT_REGISTERED) | None => Err(anyhow!(
                    "macOS did not keep TypeText registered as a login item. {}",
                    macos_startup_manual_instructions()
                )),
                Some(status) => Err(anyhow!(
                    "macOS reported an unexpected login item status ({status}). {}",
                    macos_startup_manual_instructions()
                )),
            }
        }
    }

    fn unregister_main_app_service() -> Result<()> {
        unsafe {
            let Some(service) = main_app_service() else {
                return Ok(());
            };
            match main_app_service_status() {
                Some(SM_APP_SERVICE_STATUS_NOT_REGISTERED) | None => return Ok(()),
                Some(_) => {}
            }

            let selector = sel_registerName(c"unregisterAndReturnError:".as_ptr());
            if selector.is_null() {
                return Err(anyhow!(
                    "Could not locate macOS login item unregister selector. {}",
                    macos_startup_manual_instructions()
                ));
            }

            let mut error = ptr::null_mut();
            if !objc_msg_send(service, selector, &mut error).is_null() {
                Ok(())
            } else {
                Err(anyhow!(
                    "Could not unregister TypeText from opening at login. {}",
                    macos_startup_error_details(ns_error_description(error))
                ))
            }
        }
    }

    unsafe fn main_app_service() -> Option<ObjcId> {
        let service_class = unsafe { objc_getClass(c"SMAppService".as_ptr()) };
        if service_class.is_null() {
            return None;
        }

        let selector = unsafe { sel_registerName(c"mainAppService".as_ptr()) };
        if selector.is_null() {
            return None;
        }

        let service = unsafe { objc_msg_send(service_class.cast::<c_void>(), selector) };
        if service.is_null() {
            None
        } else {
            Some(service)
        }
    }

    unsafe fn ns_error_description(error: ObjcId) -> String {
        if error.is_null() {
            return "No error details were provided by macOS.".to_string();
        }

        let description_selector = unsafe { sel_registerName(c"localizedDescription".as_ptr()) };
        let utf8_selector = unsafe { sel_registerName(c"UTF8String".as_ptr()) };
        if description_selector.is_null() || utf8_selector.is_null() {
            return "No error details were provided by macOS.".to_string();
        }

        let description = unsafe { objc_msg_send(error, description_selector) };
        if description.is_null() {
            return "No error details were provided by macOS.".to_string();
        }

        let bytes = unsafe { objc_msg_send(description, utf8_selector).cast::<c_char>() };
        if bytes.is_null() {
            "No error details were provided by macOS.".to_string()
        } else {
            unsafe { CStr::from_ptr(bytes) }
                .to_string_lossy()
                .into_owned()
        }
    }

    fn macos_startup_error_details(error: String) -> String {
        format!("{error} {}", macos_startup_manual_instructions())
    }

    fn macos_startup_manual_instructions() -> &'static str {
        "Enable it manually in System Settings > General > Login Items & Extensions > Open at Login, then turn on TypeText."
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn typing_command_keeps_script_out_of_process_arguments() {
            let command = typing_command();
            assert_eq!(command.get_program(), "osascript");
            assert!(command.get_args().next().is_none());
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
mod fallback_platform {
    use super::*;

    pub fn register_hotkey(
        _hotkey: String,
        _tx: Sender<()>,
        _repaint_ctx: eframe::egui::Context,
    ) -> Result<()> {
        Err(anyhow!(
            "Global hotkey is not implemented on this platform yet."
        ))
    }

    pub fn reregister_hotkey(_hotkey: String, _tx: Sender<()>) -> Result<()> {
        Ok(())
    }

    pub fn type_text(
        _text: &str,
        _character_delay_ms: u64,
        _separator_delay_ms: u64,
    ) -> Result<()> {
        Err(anyhow!("Typing is not implemented on this platform yet."))
    }

    pub fn type_text_current_focus(
        _text: &str,
        _character_delay_ms: u64,
        _separator_delay_ms: u64,
    ) -> Result<()> {
        Err(anyhow!("Typing is not implemented on this platform yet."))
    }

    pub fn open_folder(path: &Path) -> Result<()> {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn storage_security_warning(_path: &Path) -> Option<String> {
        None
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn open_url(url: &str) -> Result<()> {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    #[cfg(not(feature = "offline-portable"))]
    pub fn fetch_text(url: &str) -> Result<String> {
        let output = Command::new("curl")
            .args([
                "-fsSL",
                "--connect-timeout",
                "10",
                "--max-time",
                "30",
                "-H",
                "User-Agent: TypeText",
                url,
            ])
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

    pub fn open_snippets_export_dialog(_initial_dir: &Path) -> Result<Option<std::path::PathBuf>> {
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
        "Tray integration is available on the Windows and macOS builds."
    }

    pub fn install_reopen_handler(_tx: Sender<TrayCommand>, _repaint_ctx: eframe::egui::Context) {}

    pub struct TrayHandle;

    pub fn install_app_mutex() -> Result<()> {
        Ok(())
    }

    pub fn install_tray_icon(
        _tx: Sender<TrayCommand>,
        _repaint_ctx: eframe::egui::Context,
        _icon_rgba: Option<(Vec<u8>, u32, u32)>,
    ) -> Result<TrayHandle> {
        Err(anyhow!(
            "Tray integration is available on Windows and macOS."
        ))
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub use fallback_platform::{
    TrayHandle, install_app_mutex, install_reopen_handler, install_tray_icon,
    open_droptext_file_dialog, open_folder, open_snippets_export_dialog, register_hotkey,
    reregister_hotkey, set_startup_enabled, startup_enabled, storage_security_warning, tray_status,
    type_text, type_text_current_focus,
};
#[cfg(target_os = "macos")]
pub use macos_platform::{
    install_app_mutex, install_reopen_handler, open_droptext_file_dialog, open_folder,
    open_snippets_export_dialog, register_hotkey, reregister_hotkey, set_startup_enabled,
    startup_enabled, storage_security_warning, tray_status, type_text, type_text_current_focus,
};
#[cfg(windows)]
pub use windows_platform::{
    install_app_mutex, install_reopen_handler, open_droptext_file_dialog, open_folder,
    open_snippets_export_dialog, register_hotkey, reregister_hotkey, set_startup_enabled,
    startup_enabled, storage_security_warning, tray_status, type_text, type_text_current_focus,
};

#[cfg(all(
    not(feature = "offline-portable"),
    not(windows),
    not(target_os = "macos")
))]
pub use fallback_platform::{fetch_text, open_url};
#[cfg(all(not(feature = "offline-portable"), target_os = "macos"))]
pub use macos_platform::{fetch_text, open_url};
#[cfg(all(not(feature = "offline-portable"), windows))]
pub use windows_platform::{fetch_text, open_url};

#[cfg(any(windows, target_os = "macos"))]
pub use tray_integration::{TrayHandle, install_tray_icon};
