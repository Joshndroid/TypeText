use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Sender};

use crate::TrayCommand;

#[cfg(any(windows, target_os = "macos"))]
mod tray_integration {
    use super::*;
    use eframe::egui;
    use std::thread;
    use std::time::Duration;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
        Icon, TrayIcon, TrayIconBuilder,
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
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::mpsc::Receiver;
    use std::sync::OnceLock;
    use std::thread;
    use std::time::Duration;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, WPARAM};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, SendInput, UnregisterHotKey, HOT_KEY_MODIFIERS, INPUT, INPUT_0,
        INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL,
        MOD_SHIFT, MOD_WIN, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetForegroundWindow, PeekMessageW, SetForegroundWindow, TranslateMessage,
        MSG, PM_REMOVE, WM_HOTKEY,
    };

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const HOTKEY_ID: i32 = 0x5454;
    const UNICODE_INPUT_INTERVAL: Duration = Duration::from_millis(22);
    const UNICODE_WORD_BREAK_INTERVAL: Duration = Duration::from_millis(35);
    const STARTUP_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const STARTUP_RUN_VALUE: &str = "TypeText";
    static TARGET_WINDOW: AtomicIsize = AtomicIsize::new(0);
    static HOTKEY_MANAGER: OnceLock<Sender<HotkeyCommand>> = OnceLock::new();

    pub fn install_app_mutex() -> Result<()> {
        unsafe {
            CreateMutexW(None, false, w!("TypeTextAppMutex"))
                .context("Could not create TypeText app mutex")?;
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

    pub fn type_text(text: &str) -> Result<()> {
        restore_target_window();
        send_text(text)
    }

    pub fn type_text_current_focus(text: &str) -> Result<()> {
        send_text(text)
    }

    fn send_text(text: &str) -> Result<()> {
        release_modifier_keys()?;
        thread::sleep(Duration::from_millis(20));
        for unit in text.encode_utf16() {
            send_unicode_unit(unit)?;
            thread::sleep(unicode_input_interval(unit));
        }
        Ok(())
    }

    pub fn remember_target_window() {
        let hwnd = unsafe { GetForegroundWindow() };
        TARGET_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
    }

    pub fn startup_enabled() -> bool {
        startup_registry_enabled()
            || startup_shortcut_path().is_some_and(|path| path.exists())
            || legacy_startup_script_path().is_some_and(|path| path.exists())
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<()> {
        if enabled {
            let exe =
                std::env::current_exe().context("Could not determine current executable path")?;
            write_startup_registry_entry(&exe)?;
        } else {
            remove_startup_registry_entry()?;
        }

        remove_legacy_startup_files()?;
        Ok(())
    }

    pub fn tray_status() -> &'static str {
        "TypeText stays running when hidden or closed, and re-opens from its tray icon or global hotkey. Use Exit in the tray menu or Quit here to close it."
    }

    pub fn install_reopen_handler(_tx: Sender<TrayCommand>, _repaint_ctx: eframe::egui::Context) {}

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

    fn unicode_input_interval(unit: u16) -> Duration {
        match char::from_u32(unit as u32) {
            Some(' ' | '\t' | '\n' | '\r' | '.' | ',' | ';' | ':' | '!' | '?') => {
                UNICODE_WORD_BREAK_INTERVAL
            }
            _ => UNICODE_INPUT_INTERVAL,
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
        let mut command = hidden_command("explorer");
        command.arg(path).spawn().map(|_| ()).map_err(Into::into)
    }

    pub fn open_url(url: &str) -> Result<()> {
        let mut command = hidden_command("rundll32");
        command
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn fetch_text(url: &str) -> Result<String> {
        let output = hidden_command("powershell")
            .env("TYPETEXT_UPDATE_URL", url)
            .args([
                "-NoProfile",
                "-Command",
                "$ProgressPreference='SilentlyContinue'; $uri=[Environment]::GetEnvironmentVariable('TYPETEXT_UPDATE_URL'); (Invoke-WebRequest -UseBasicParsing -Headers @{'User-Agent'='TypeText'} -Uri $uri).Content",
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
$dialog.Title = 'Import DropText.ini'
$dialog.Filter = 'DropText INI (*.ini)|*.ini|All files (*.*)|*.*'
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

    fn release_modifier_keys() -> Result<()> {
        for key in [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN] {
            send_key_up(key)?;
        }
        Ok(())
    }

    fn send_key_up(key: VIRTUAL_KEY) -> Result<()> {
        let mut inputs = [INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }];

        let sent = unsafe { SendInput(&mut inputs, size_of::<INPUT>() as i32) };
        if sent == 0 {
            Err(anyhow!("SendInput failed"))
        } else {
            Ok(())
        }
    }

    fn restore_target_window() {
        let raw = TARGET_WINDOW.load(Ordering::Relaxed);
        if raw != 0 {
            let _ = unsafe { SetForegroundWindow(HWND(raw as *mut std::ffi::c_void)) };
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }

    fn startup_registry_enabled() -> bool {
        hidden_command("reg")
            .args(["query", STARTUP_RUN_KEY, "/v", STARTUP_RUN_VALUE])
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn write_startup_registry_entry(exe_path: &Path) -> Result<()> {
        let value = format!("\"{}\"", exe_path.display());
        let output = hidden_command("reg")
            .args([
                "add",
                STARTUP_RUN_KEY,
                "/v",
                STARTUP_RUN_VALUE,
                "/t",
                "REG_SZ",
                "/d",
                &value,
                "/f",
            ])
            .output()
            .context("Could not create Windows startup entry")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "Windows startup entry creation failed. {}",
                stderr.trim()
            ));
        }

        Ok(())
    }

    fn remove_startup_registry_entry() -> Result<()> {
        let output = hidden_command("reg")
            .args(["delete", STARTUP_RUN_KEY, "/v", STARTUP_RUN_VALUE, "/f"])
            .output()
            .context("Could not remove Windows startup entry")?;

        if !output.status.success() && startup_registry_enabled() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "Windows startup entry removal failed. {}",
                stderr.trim()
            ));
        }

        Ok(())
    }

    fn remove_legacy_startup_files() -> Result<()> {
        for path in [startup_shortcut_path(), legacy_startup_script_path()]
            .into_iter()
            .flatten()
        {
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("Could not remove {}", path.display()))?;
            }
        }
        Ok(())
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

    fn hidden_command(program: &str) -> Command {
        let mut command = Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
}

#[cfg(target_os = "macos")]
mod macos_platform {
    use super::*;
    use eframe::egui;
    use std::ffi::c_char;
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
        fn UnregisterEventHotKey(hot_key_ref: EventHotKeyRef) -> OSStatus;
        fn RunApplicationEventLoop();
    }

    #[link(name = "objc", kind = "dylib")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> ObjcClass;
        fn sel_registerName(name: *const c_char) -> ObjcSel;
        fn class_addMethod(
            class: ObjcClass,
            name: ObjcSel,
            imp: ObjcImp,
            types: *const c_char,
        ) -> bool;
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
        Ok(())
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
        let register_status =
            RegisterEventHotKey(key_code, modifiers, hotkey_id, target, 0, &mut hotkey_ref);
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
                "macOS typing failed because TypeText needs Accessibility permission to send keyboard input. Open System Settings > Privacy & Security > Accessibility, then enable the terminal app that launched TypeText during development, or enable TypeText when running the packaged app. {}",
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
    fetch_text, install_app_mutex, install_reopen_handler, install_tray_icon,
    open_droptext_file_dialog, open_folder, open_snippets_export_dialog, open_url, register_hotkey,
    reregister_hotkey, set_startup_enabled, startup_enabled, tray_status, type_text,
    type_text_current_focus, TrayHandle,
};
#[cfg(target_os = "macos")]
pub use macos_platform::{
    fetch_text, install_app_mutex, install_reopen_handler, open_droptext_file_dialog, open_folder,
    open_snippets_export_dialog, open_url, register_hotkey, reregister_hotkey, set_startup_enabled,
    startup_enabled, tray_status, type_text, type_text_current_focus,
};
#[cfg(windows)]
pub use windows_platform::{
    fetch_text, install_app_mutex, install_reopen_handler, open_droptext_file_dialog, open_folder,
    open_snippets_export_dialog, open_url, register_hotkey, reregister_hotkey, set_startup_enabled,
    startup_enabled, tray_status, type_text, type_text_current_focus,
};

#[cfg(any(windows, target_os = "macos"))]
pub use tray_integration::{install_tray_icon, TrayHandle};
