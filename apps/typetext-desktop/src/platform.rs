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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    OpenChooser,
    InsertFavourite(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub action: HotkeyAction,
    pub hotkey: String,
}

impl HotkeyAction {
    fn id(self) -> u32 {
        match self {
            Self::OpenChooser => 1,
            Self::InsertFavourite(slot) => u32::from(slot) + 1,
        }
    }

    fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::OpenChooser),
            2..=11 => Some(Self::InsertFavourite((id - 1) as u8)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod hotkey_action_tests {
    use super::HotkeyAction;

    #[test]
    fn numbered_hotkey_actions_round_trip_through_platform_ids() {
        assert_eq!(HotkeyAction::from_id(1), Some(HotkeyAction::OpenChooser));
        for slot in 1..=10 {
            let action = HotkeyAction::InsertFavourite(slot);
            assert_eq!(HotkeyAction::from_id(action.id()), Some(action));
        }
        assert_eq!(HotkeyAction::from_id(12), None);
    }
}

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
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::mpsc::Receiver;
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_CANCELLED, GetLastError, HWND,
    };
    #[cfg(not(feature = "offline-portable"))]
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    #[cfg(not(feature = "offline-portable"))]
    use windows::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
        WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen,
        WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    };
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
    };
    #[cfg(not(feature = "offline-portable"))]
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS,
        REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW,
    };
    use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
        SendInput, UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN,
        VK_SHIFT, VK_SPACE, VK_TAB,
    };
    use windows::Win32::UI::Shell::{
        Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT,
        FOS_PATHMUSTEXIST, FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog,
        IFileSaveDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH, ShellExecuteW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetForegroundWindow, GetWindowThreadProcessId, IsWindow, MSG, PM_REMOVE,
        PeekMessageW, SW_SHOWNORMAL, SetForegroundWindow, TranslateMessage, WM_HOTKEY,
    };
    use windows::core::{HRESULT, PCWSTR, w};

    const DRIVE_REMOTE_TYPE: u32 = 4;
    const HOTKEY_ID_BASE: i32 = 0x5454;
    #[cfg(not(feature = "offline-portable"))]
    const STARTUP_RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    #[cfg(not(feature = "offline-portable"))]
    const STARTUP_RUN_VALUE: &str = "TypeText";
    static APP_MUTEX_HANDLE: AtomicIsize = AtomicIsize::new(0);
    static TARGET_WINDOW: AtomicIsize = AtomicIsize::new(0);
    static TARGET_PROCESS_ID: AtomicU32 = AtomicU32::new(0);
    static HOTKEY_MANAGER: OnceLock<Sender<HotkeyCommand>> = OnceLock::new();

    /// Restrict later DLL loads to Windows' trusted system directory.
    ///
    /// TypeText is a single-executable application and does not ship companion
    /// DLLs. Excluding the executable directory, current directory, and PATH
    /// prevents DLL preloading when a portable build runs from a user-writable
    /// folder. This must run before eframe/WGPU or the tray integration starts.
    pub fn harden_dll_search() -> Result<()> {
        unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) }
            .context("Could not restrict DLL loading to the Windows system directory")
    }

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
            bindings: Vec<HotkeyBinding>,
            reply_tx: Sender<Result<(), String>>,
        },
    }

    #[derive(Clone)]
    struct ActiveBinding {
        binding: HotkeyBinding,
        modifiers: HOT_KEY_MODIFIERS,
        key: u32,
    }

    struct ActiveHotkeys {
        bindings: Vec<ActiveBinding>,
        tx: Sender<HotkeyAction>,
        repaint_ctx: eframe::egui::Context,
    }

    pub fn register_hotkeys(
        bindings: Vec<HotkeyBinding>,
        tx: Sender<HotkeyAction>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<()> {
        let bindings = parse_bindings(bindings).map_err(|error| anyhow!(error))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        thread::spawn(move || {
            let active = ActiveHotkeys {
                bindings,
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

    pub fn reregister_hotkeys(
        bindings: Vec<HotkeyBinding>,
        _tx: Sender<HotkeyAction>,
    ) -> Result<()> {
        let manager = HOTKEY_MANAGER
            .get()
            .ok_or_else(|| anyhow!("Hotkey registration thread is not running"))?;
        let (reply_tx, reply_rx) = mpsc::channel();
        manager
            .send(HotkeyCommand::Reregister { bindings, reply_tx })
            .map_err(|_| anyhow!("Hotkey registration thread stopped"))?;
        reply_rx
            .recv()
            .unwrap_or_else(|_| Err("Hotkey registration thread stopped".to_string()))
            .map_err(|error| anyhow!(error))
    }

    fn run_hotkey_manager(
        mut active: ActiveHotkeys,
        command_rx: Receiver<HotkeyCommand>,
        ready_tx: Sender<Result<(), String>>,
    ) {
        if let Err(error) = register_bindings(&active.bindings) {
            let _ = ready_tx.send(Err(error));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        loop {
            while let Ok(command) = command_rx.try_recv() {
                match command {
                    HotkeyCommand::Reregister { bindings, reply_tx } => {
                        let result = replace_hotkeys(&mut active, bindings);
                        let _ = reply_tx.send(result);
                    }
                }
            }

            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                    if msg.message == WM_HOTKEY
                        && let Some(action) = hotkey_action_from_windows_id(msg.wParam.0)
                    {
                        remember_target_window();
                        let _ = active.tx.send(action);
                        active.repaint_ctx.request_repaint();
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            thread::sleep(Duration::from_millis(20));
        }
    }

    fn parse_bindings(bindings: Vec<HotkeyBinding>) -> Result<Vec<ActiveBinding>, String> {
        bindings
            .into_iter()
            .map(|binding| {
                let (modifiers, key) = parse_hotkey(&binding.hotkey)
                    .ok_or_else(|| format!("Invalid hotkey: {}", binding.hotkey))?;
                Ok(ActiveBinding {
                    binding,
                    modifiers,
                    key,
                })
            })
            .collect()
    }

    fn windows_hotkey_id(action: HotkeyAction) -> i32 {
        HOTKEY_ID_BASE + action.id() as i32
    }

    fn hotkey_action_from_windows_id(id: usize) -> Option<HotkeyAction> {
        let offset = i32::try_from(id).ok()?.checked_sub(HOTKEY_ID_BASE)?;
        HotkeyAction::from_id(u32::try_from(offset).ok()?)
    }

    fn unregister_bindings(bindings: &[ActiveBinding]) {
        for binding in bindings {
            unsafe {
                let _ = UnregisterHotKey(None, windows_hotkey_id(binding.binding.action));
            }
        }
    }

    fn register_bindings(bindings: &[ActiveBinding]) -> Result<(), String> {
        let mut registered = Vec::new();
        for binding in bindings {
            let result = unsafe {
                RegisterHotKey(
                    None,
                    windows_hotkey_id(binding.binding.action),
                    HOT_KEY_MODIFIERS(binding.modifiers.0 | MOD_NOREPEAT.0),
                    binding.key,
                )
            };
            if let Err(error) = result {
                unregister_bindings(&registered);
                return Err(format!(
                    "Could not register {}: {error}",
                    binding.binding.hotkey
                ));
            }
            registered.push(binding.clone());
        }
        Ok(())
    }

    fn replace_hotkeys(
        active: &mut ActiveHotkeys,
        bindings: Vec<HotkeyBinding>,
    ) -> Result<(), String> {
        let requested = parse_bindings(bindings)?;
        let previous = active.bindings.clone();
        unregister_bindings(&previous);
        if let Err(error) = register_bindings(&requested) {
            register_bindings(&previous)
                .map_err(|restore| format!("{error}; could not restore hotkeys: {restore}"))?;
            return Err(error);
        }
        active.bindings = requested;
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
        let mut process_id = 0u32;
        if !hwnd.0.is_null() {
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        }
        if process_id == unsafe { GetCurrentProcessId() } {
            return;
        }
        TARGET_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
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

    /// Open an HTTPS link in the default browser via `ShellExecuteW`.
    ///
    /// Deliberately does NOT spawn `rundll32 url.dll,FileProtocolHandler`:
    /// rundll32 is a LOLBin that AV behavioural heuristics flag, and
    /// `FileProtocolHandler` will execute local or UNC paths. The scheme check
    /// keeps this from ever launching anything but a web link, even if a
    /// future caller passes an unvalidated URL.
    #[cfg(not(feature = "offline-portable"))]
    pub fn open_url(url: &str) -> Result<()> {
        let parsed = url::Url::parse(url).context("Invalid URL")?;
        anyhow::ensure!(parsed.scheme() == "https", "Only HTTPS links can be opened");

        let url_wide = wide_null(url);
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(url_wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            Err(anyhow!("Windows could not open the link"))
        } else {
            Ok(())
        }
    }

    /// Owns a WinHTTP handle and closes it when dropped, including on the
    /// early-error paths in [`fetch_text`].
    #[cfg(not(feature = "offline-portable"))]
    struct WinHttpHandle(*mut std::ffi::c_void);

    #[cfg(not(feature = "offline-portable"))]
    impl Drop for WinHttpHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let _ = unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }

    /// Fetch a small HTTPS text resource in-process via WinHTTP.
    ///
    /// Deliberately does NOT shell out to PowerShell or curl: an unsigned
    /// executable spawning a hidden interpreter that then opens a network
    /// connection is a classic AV behavioural-detection chain, and PowerShell
    /// may be blocked outright by AppLocker or Constrained Language Mode.
    /// WinHTTP performs OS certificate validation and blocks HTTPS->HTTP
    /// redirect downgrades by default.
    #[cfg(not(feature = "offline-portable"))]
    pub fn fetch_text(url: &str) -> Result<String> {
        const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

        let parsed = url::Url::parse(url).context("Invalid update URL")?;
        anyhow::ensure!(parsed.scheme() == "https", "Update requests must use HTTPS");
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow!("Update URL is missing a host"))?;
        let port = parsed.port().unwrap_or(443);
        let mut object = parsed.path().to_string();
        if let Some(query) = parsed.query() {
            object.push('?');
            object.push_str(query);
        }
        let host_wide = wide_null(host);
        let object_wide = wide_null(&object);

        unsafe {
            let session = WinHttpHandle(WinHttpOpen(
                w!("TypeText"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ));
            anyhow::ensure!(!session.0.is_null(), "Could not open a WinHTTP session");
            WinHttpSetTimeouts(session.0, 10_000, 10_000, 30_000, 30_000)
                .context("Could not set update request timeouts")?;

            let connection = WinHttpHandle(WinHttpConnect(
                session.0,
                PCWSTR(host_wide.as_ptr()),
                port,
                0,
            ));
            anyhow::ensure!(
                !connection.0.is_null(),
                "Could not connect for the update check"
            );

            let request = WinHttpHandle(WinHttpOpenRequest(
                connection.0,
                w!("GET"),
                PCWSTR(object_wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ));
            anyhow::ensure!(!request.0.is_null(), "Could not create the update request");

            WinHttpSendRequest(request.0, None, None, 0, 0, 0)
                .context("Update request failed to send")?;
            WinHttpReceiveResponse(request.0, std::ptr::null_mut())
                .context("Update request received no response")?;

            let mut status_code = 0u32;
            let mut status_size = size_of::<u32>() as u32;
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&mut status_code as *mut u32).cast()),
                &mut status_size,
                std::ptr::null_mut(),
            )
            .context("Could not read the update response status")?;
            anyhow::ensure!(
                (200..300).contains(&status_code),
                "Update request failed. HTTP status {status_code}"
            );

            let mut body: Vec<u8> = Vec::new();
            loop {
                let mut available = 0u32;
                WinHttpQueryDataAvailable(request.0, &mut available)
                    .context("Could not read the update response")?;
                if available == 0 {
                    break;
                }
                anyhow::ensure!(
                    body.len().saturating_add(available as usize) <= MAX_RESPONSE_BYTES,
                    "Update response exceeds the {MAX_RESPONSE_BYTES} byte safety limit"
                );
                let mut chunk = vec![0u8; available as usize];
                let mut read = 0u32;
                WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), available, &mut read)
                    .context("Could not read the update response")?;
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read as usize]);
            }

            Ok(String::from_utf8_lossy(&body).to_string())
        }
    }

    /// Balances `CoInitializeEx` with `CoUninitialize` on drop. When COM is
    /// already initialized in an incompatible mode (`RPC_E_CHANGED_MODE`),
    /// initialization is not balanced but COM remains usable.
    struct ComInit {
        should_uninit: bool,
    }

    impl ComInit {
        fn new() -> Self {
            let result =
                unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
            Self {
                should_uninit: result.is_ok(),
            }
        }
    }

    impl Drop for ComInit {
        fn drop(&mut self) {
            if self.should_uninit {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Native `IFileOpenDialog` replacement for the previous PowerShell
    /// WinForms dialog: no child process, no interpreter, works under
    /// AppLocker and Constrained Language Mode.
    pub fn open_droptext_file_dialog() -> Result<Option<PathBuf>> {
        let _com = ComInit::new();
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
                .context("Could not open Windows file dialog")?;
        unsafe {
            dialog
                .SetTitle(w!("Import DropText snippets"))
                .and_then(|_| {
                    dialog.SetFileTypes(&[
                        COMDLG_FILTERSPEC {
                            pszName: w!("DropText files (*.ini;*.csv)"),
                            pszSpec: w!("*.ini;*.csv"),
                        },
                        COMDLG_FILTERSPEC {
                            pszName: w!("DropText INI (*.ini)"),
                            pszSpec: w!("*.ini"),
                        },
                        COMDLG_FILTERSPEC {
                            pszName: w!("DropText CSV (*.csv)"),
                            pszSpec: w!("*.csv"),
                        },
                        COMDLG_FILTERSPEC {
                            pszName: w!("All files (*.*)"),
                            pszSpec: w!("*.*"),
                        },
                    ])
                })
                .and_then(|_| dialog.GetOptions())
                .and_then(|options| {
                    dialog.SetOptions(
                        options | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM,
                    )
                })
                .context("Could not configure Windows file dialog")?;
        }

        show_dialog_and_take_path(&dialog.into(), "Windows file dialog failed")
    }

    /// Native `IFileSaveDialog` replacement for the previous PowerShell
    /// WinForms dialog; see [`open_droptext_file_dialog`].
    pub fn open_snippets_export_dialog(initial_dir: &Path) -> Result<Option<PathBuf>> {
        let _com = ComInit::new();
        let dialog: IFileSaveDialog =
            unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }
                .context("Could not open Windows save dialog")?;
        unsafe {
            dialog
                .SetTitle(w!("Export TypeText snippets"))
                .and_then(|_| {
                    dialog.SetFileTypes(&[
                        COMDLG_FILTERSPEC {
                            pszName: w!("TypeText snippets (*.json)"),
                            pszSpec: w!("*.json"),
                        },
                        COMDLG_FILTERSPEC {
                            pszName: w!("All files (*.*)"),
                            pszSpec: w!("*.*"),
                        },
                    ])
                })
                .and_then(|_| dialog.SetFileName(w!("snippets.json")))
                .and_then(|_| dialog.SetDefaultExtension(w!("json")))
                .and_then(|_| dialog.GetOptions())
                .and_then(|options| {
                    dialog.SetOptions(
                        options | FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM,
                    )
                })
                .context("Could not configure Windows save dialog")?;

            let initial_dir_wide = wide_null(&initial_dir.to_string_lossy());
            let folder: windows::core::Result<IShellItem> =
                SHCreateItemFromParsingName(PCWSTR(initial_dir_wide.as_ptr()), None);
            if let Ok(folder) = folder {
                let _ = dialog.SetFolder(&folder);
            }
        }

        show_dialog_and_take_path(&dialog.into(), "Windows save dialog failed")
    }

    fn show_dialog_and_take_path(
        dialog: &IFileDialog,
        failure_message: &str,
    ) -> Result<Option<PathBuf>> {
        match unsafe { dialog.Show(None) } {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => return Err(anyhow!("{failure_message}. {error}")),
        }

        let item =
            unsafe { dialog.GetResult() }.map_err(|error| anyhow!("{failure_message}. {error}"))?;
        let path = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|error| anyhow!("{failure_message}. {error}"))?;
        let text = unsafe { path.to_string() };
        unsafe { CoTaskMemFree(Some(path.as_ptr() as *const _)) };
        let text = text.map_err(|error| anyhow!("{failure_message}. {error}"))?;

        if text.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(text)))
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
        fn GetEventParameter(
            event: EventRef,
            name: UInt32,
            desired_type: UInt32,
            actual_type: *mut UInt32,
            buffer_size: usize,
            actual_size: *mut usize,
            data: *mut c_void,
        ) -> OSStatus;
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
    const K_EVENT_CLASS_KEYBOARD: UInt32 = u32::from_be_bytes(*b"keyb");
    const K_EVENT_HOT_KEY_PRESSED: UInt32 = 5;
    const K_EVENT_PARAM_DIRECT_OBJECT: UInt32 = u32::from_be_bytes(*b"----");
    const TYPE_EVENT_HOT_KEY_ID: UInt32 = u32::from_be_bytes(*b"hkid");

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
    static HOTKEY_STATE: OnceLock<Mutex<Vec<ActiveHotkey>>> = OnceLock::new();

    #[derive(Clone)]
    struct ActiveHotkey {
        action: HotkeyAction,
        hotkey: String,
        modifiers: UInt32,
        key_code: UInt32,
        hotkey_ref: usize,
    }

    pub fn register_hotkeys(
        bindings: Vec<HotkeyBinding>,
        tx: Sender<HotkeyAction>,
        repaint_ctx: eframe::egui::Context,
    ) -> Result<()> {
        let parsed = parse_carbon_bindings(&bindings)?;

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

            match register_carbon_bindings(&parsed, target) {
                Ok(active) => {
                    let state = HOTKEY_STATE.get_or_init(|| Mutex::new(Vec::new()));
                    *state.lock().expect("hotkey state poisoned") = active;
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
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

    pub fn reregister_hotkeys(
        bindings: Vec<HotkeyBinding>,
        _tx: Sender<HotkeyAction>,
    ) -> Result<()> {
        let parsed = parse_carbon_bindings(&bindings)?;
        let state = HOTKEY_STATE
            .get()
            .ok_or_else(|| anyhow!("Hotkey registration thread is not running"))?;
        let mut state = state.lock().expect("hotkey state poisoned");
        let previous = state.clone();

        unsafe {
            for active in &previous {
                let status = UnregisterEventHotKey(active.hotkey_ref as EventHotKeyRef);
                if status != NO_ERR {
                    return Err(anyhow!("UnregisterEventHotKey failed with status {status}"));
                }
            }

            let target = GetApplicationEventTarget();
            match register_carbon_bindings(&parsed, target) {
                Ok(active) => {
                    *state = active;
                    Ok(())
                }
                Err(error) => {
                    let restore = previous
                        .iter()
                        .map(|active| {
                            (
                                active.action,
                                active.hotkey.clone(),
                                active.key_code,
                                active.modifiers,
                            )
                        })
                        .collect::<Vec<_>>();
                    match register_carbon_bindings(&restore, target) {
                        Ok(restored) => {
                            *state = restored;
                            Err(anyhow!(error))
                        }
                        Err(restore_error) => Err(anyhow!(
                            "{error}; could not restore hotkeys: {restore_error}"
                        )),
                    }
                }
            }
        }
    }

    fn parse_carbon_bindings(
        bindings: &[HotkeyBinding],
    ) -> Result<Vec<(HotkeyAction, String, UInt32, UInt32)>> {
        bindings
            .iter()
            .map(|binding| {
                let (modifiers, key_code) = parse_hotkey(&binding.hotkey)
                    .ok_or_else(|| anyhow!("Invalid hotkey: {}", binding.hotkey))?;
                Ok((binding.action, binding.hotkey.clone(), key_code, modifiers))
            })
            .collect()
    }

    unsafe fn register_carbon_bindings(
        bindings: &[(HotkeyAction, String, UInt32, UInt32)],
        target: EventTargetRef,
    ) -> std::result::Result<Vec<ActiveHotkey>, String> {
        let mut active = Vec::new();
        for (action, hotkey, key_code, modifiers) in bindings {
            match unsafe { register_carbon_hotkey(*action, *key_code, *modifiers, target) } {
                Ok(hotkey_ref) => active.push(ActiveHotkey {
                    action: *action,
                    hotkey: hotkey.clone(),
                    modifiers: *modifiers,
                    key_code: *key_code,
                    hotkey_ref: hotkey_ref as usize,
                }),
                Err(status) => {
                    for registered in &active {
                        unsafe {
                            let _ = UnregisterEventHotKey(registered.hotkey_ref as EventHotKeyRef);
                        }
                    }
                    return Err(format!("Could not register {hotkey}: status {status}"));
                }
            }
        }
        Ok(active)
    }

    unsafe fn register_carbon_hotkey(
        action: HotkeyAction,
        key_code: UInt32,
        modifiers: UInt32,
        target: EventTargetRef,
    ) -> std::result::Result<EventHotKeyRef, OSStatus> {
        let hotkey_id = EventHotKeyID {
            signature: HOTKEY_SIGNATURE,
            id: action.id(),
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

    /// Open an HTTPS link in the default browser. The scheme check keeps
    /// `open` from ever launching local files or applications, even if a
    /// future caller passes an unvalidated URL.
    #[cfg(not(feature = "offline-portable"))]
    pub fn open_url(url: &str) -> Result<()> {
        let parsed = url::Url::parse(url).context("Invalid URL")?;
        anyhow::ensure!(parsed.scheme() == "https", "Only HTTPS links can be opened");

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
        event: EventRef,
        user_data: *mut c_void,
    ) -> OSStatus {
        let mut hotkey_id = EventHotKeyID {
            signature: 0,
            id: 0,
        };
        let status = unsafe {
            GetEventParameter(
                event,
                K_EVENT_PARAM_DIRECT_OBJECT,
                TYPE_EVENT_HOT_KEY_ID,
                ptr::null_mut(),
                std::mem::size_of::<EventHotKeyID>(),
                ptr::null_mut(),
                (&mut hotkey_id as *mut EventHotKeyID).cast::<c_void>(),
            )
        };
        if status != NO_ERR || hotkey_id.signature != HOTKEY_SIGNATURE {
            return status;
        }
        let Some(action) = HotkeyAction::from_id(hotkey_id.id) else {
            return NO_ERR;
        };
        remember_target_application();
        let (tx, repaint_ctx) =
            unsafe { &*(user_data.cast::<(Sender<HotkeyAction>, eframe::egui::Context)>()) };
        let _ = tx.send(action);
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

    pub fn register_hotkeys(
        _bindings: Vec<HotkeyBinding>,
        _tx: Sender<HotkeyAction>,
        _repaint_ctx: eframe::egui::Context,
    ) -> Result<()> {
        Err(anyhow!(
            "Global hotkey is not implemented on this platform yet."
        ))
    }

    pub fn reregister_hotkeys(
        _bindings: Vec<HotkeyBinding>,
        _tx: Sender<HotkeyAction>,
    ) -> Result<()> {
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

    /// Open an HTTPS link in the default browser. The scheme check keeps
    /// `xdg-open` from ever launching local files or applications, even if a
    /// future caller passes an unvalidated URL.
    #[cfg(not(feature = "offline-portable"))]
    pub fn open_url(url: &str) -> Result<()> {
        let parsed = url::Url::parse(url).context("Invalid URL")?;
        anyhow::ensure!(parsed.scheme() == "https", "Only HTTPS links can be opened");

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
    open_droptext_file_dialog, open_folder, open_snippets_export_dialog, register_hotkeys,
    reregister_hotkeys, set_startup_enabled, startup_enabled, storage_security_warning,
    tray_status, type_text, type_text_current_focus,
};
#[cfg(target_os = "macos")]
pub use macos_platform::{
    install_app_mutex, install_reopen_handler, open_droptext_file_dialog, open_folder,
    open_snippets_export_dialog, register_hotkeys, reregister_hotkeys, set_startup_enabled,
    startup_enabled, storage_security_warning, tray_status, type_text, type_text_current_focus,
};
#[cfg(windows)]
pub use windows_platform::{
    harden_dll_search, install_app_mutex, install_reopen_handler, open_droptext_file_dialog,
    open_folder, open_snippets_export_dialog, register_hotkeys, reregister_hotkeys,
    set_startup_enabled, startup_enabled, storage_security_warning, tray_status, type_text,
    type_text_current_focus,
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
