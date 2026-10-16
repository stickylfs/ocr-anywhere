#![windows_subsystem = "windows"]

use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadIconW,
    PostQuitMessage, RegisterClassExW, TranslateMessage, CREATESTRUCTW, MSG, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
    WNDCLASSEXW,
};

use ocr_anywhere::app::{capture_in_progress, AppCoordinator, CaptureOutcome};
use ocr_anywhere::history::HistoryStore;
use ocr_anywhere::hotkey::{HotkeyManager, OCR_HOTKEY_ID};
use ocr_anywhere::settings::storage::set_windows_startup;
use ocr_anywhere::settings::{AppSettings, Language};
use ocr_anywhere::tray::{
    taskbar_created_message, TrayIcon, TRAY_MENU_EXIT, TRAY_MENU_HISTORY, TRAY_MENU_OCR,
    TRAY_MENU_SETTINGS, WM_TRAY_ICON,
};
use ocr_anywhere::ui::alert::{show_alert, Alert, AlertAction};
use ocr_anywhere::ui::dialog::{task_dialog, task_dialog_with_details, DialogIcon};
use ocr_anywhere::ui::{show_history_window, show_settings_window};
use ocr_anywhere::{set_host_window, WM_APP_SETTINGS_CHANGED};

const BACKGROUND_CLASS_NAME: PCWSTR = w!("OCRAnywhere_BackgroundClass");
/// Per-session single-instance mutex (`Local\`, so other signed-in users can
/// run their own instance).
const SINGLE_INSTANCE_MUTEX: PCWSTR = w!("Local\\OCRAnywhere_SingleInstanceMutex");

struct MainAppState {
    coordinator: AppCoordinator,
    hotkey_mgr: HotkeyManager,
    tray: Option<TrayIcon>,
}

impl MainAppState {
    fn language(&self) -> Language {
        Language::resolve(self.coordinator.settings.lock().unwrap().language)
    }

    fn hotkey_label(&self) -> String {
        self.coordinator
            .settings
            .lock()
            .unwrap()
            .hotkey
            .display_string()
    }

    fn notify(&mut self, text: &str, warning: bool) {
        if let Some(tray) = self.tray.as_mut() {
            tray.notify("OCR Anywhere", text, warning);
        }
    }

    /// Shows `alert`, or a tray balloon with `fallback` when another alert is open.
    fn alert(&mut self, alert: Alert, fallback: &str) {
        if !show_alert(alert) {
            self.notify(fallback, true);
        }
    }

    /// (Re-)registers the global hotkey. A conflict makes the app useless, so it
    /// is reported with a dialog that leads to the shortcut setting.
    fn register_hotkey(&mut self) {
        let hotkey = self.coordinator.settings.lock().unwrap().hotkey.clone();
        if self.hotkey_mgr.register(&hotkey).is_err() {
            let lang = self.language();
            let label = hotkey.display_string();
            self.alert(
                Alert {
                    instruction: lang.hotkey_conflict_title(&label),
                    content: lang.hotkey_conflict_content().to_string(),
                    details: String::new(),
                    icon: DialogIcon::Warning,
                    action: Some((
                        lang.change_shortcut().to_string(),
                        AlertAction::OpenSettings,
                    )),
                    close_label: lang.alert_close().to_string(),
                },
                &lang.hotkey_in_use(&label),
            );
        }
    }

    /// Keeps the "start with Windows" entry pointing at the current executable.
    fn sync_startup_entry(&mut self) {
        let enabled = self.coordinator.settings.lock().unwrap().start_with_windows;
        if let Ok(exe) = std::env::current_exe() {
            if set_windows_startup(enabled, &exe).is_err() && enabled {
                let text = self.language().startup_entry_failed();
                self.notify(text, true);
            }
        }
    }

    fn capture(&mut self) {
        let lang = self.language();
        match self.coordinator.trigger_ocr_pipeline() {
            CaptureOutcome::NoText => self.notify(lang.no_text_found(), false),
            CaptureOutcome::CopiedHistoryFailed(_) => self.notify(lang.history_save_failed(), true),
            CaptureOutcome::NoOcrLanguages => self.alert(
                Alert {
                    instruction: lang.no_ocr_languages_title().to_string(),
                    content: lang.no_ocr_languages_content().to_string(),
                    details: String::new(),
                    icon: DialogIcon::Warning,
                    action: Some((
                        lang.open_language_settings().to_string(),
                        AlertAction::Open("ms-settings:regionlanguage"),
                    )),
                    close_label: lang.alert_close().to_string(),
                },
                lang.no_ocr_languages(),
            ),
            CaptureOutcome::ClipboardFailed {
                error,
                saved_in_history,
            } => {
                let mut content = lang.clipboard_failed_content().to_string();
                if saved_in_history {
                    content.push_str("\n\n");
                    content.push_str(lang.clipboard_saved_in_history());
                }
                self.alert(
                    Alert {
                        instruction: lang.clipboard_failed().to_string(),
                        content,
                        details: error,
                        icon: DialogIcon::Warning,
                        action: None,
                        close_label: lang.alert_close().to_string(),
                    },
                    lang.clipboard_failed(),
                );
            }
            CaptureOutcome::Failed(error) => self.alert(
                Alert {
                    instruction: lang.capture_failed().to_string(),
                    content: lang.capture_failed_content().to_string(),
                    details: error,
                    icon: DialogIcon::Error,
                    action: None,
                    close_label: lang.alert_close().to_string(),
                },
                lang.capture_failed(),
            ),
            CaptureOutcome::Copied(_) | CaptureOutcome::Cancelled | CaptureOutcome::Busy => {}
        }
    }

    fn on_settings_changed(&mut self) {
        self.coordinator.apply_settings();
        self.register_hotkey();
        self.sync_startup_entry();
        let tooltip = self.language().tray_tooltip(&self.hotkey_label());
        if let Some(tray) = self.tray.as_mut() {
            tray.set_tooltip(&tooltip);
        }
    }
}

fn main() {
    unsafe {
        // Must run before any window is created.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let settings = AppSettings::load();
        let lang = Language::resolve(settings.language);

        let mutex = match CreateMutexW(None, true, SINGLE_INSTANCE_MUTEX) {
            Ok(handle) => handle,
            Err(_) => return,
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            task_dialog(
                "OCR Anywhere",
                lang.already_running_title(),
                &lang.already_running_content(&settings.hotkey.display_string()),
                &[],
                0,
                DialogIcon::App,
            );
            let _ = CloseHandle(mutex);
            return;
        }

        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            show_startup_error(lang, &e.to_string());
            return;
        }

        let max_entries = settings.max_history_entries;
        let settings = Arc::new(Mutex::new(settings));
        let history = Arc::new(Mutex::new(HistoryStore::new(
            ocr_anywhere::history::default_history_path(),
            max_entries,
        )));

        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        #[allow(clippy::manual_dangling_ptr)]
        let app_icon = LoadIconW(hinstance, PCWSTR(1 as *const u16)).unwrap_or_default();

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(main_wnd_proc),
            hInstance: hinstance.into(),
            hIcon: app_icon,
            lpszClassName: BACKGROUND_CLASS_NAME,
            hIconSm: app_icon,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wnd_class);
        let lang_for_errors = lang;

        let state_ptr = Box::into_raw(Box::new(MainAppState {
            coordinator: AppCoordinator::new(Arc::clone(&settings), history),
            hotkey_mgr: HotkeyManager::new(HWND::default()),
            tray: None,
        }));

        // A hidden top-level window (not message-only) so it receives the
        // broadcast `TaskbarCreated` message.
        if CreateWindowExW(
            WINDOW_EX_STYLE(0),
            BACKGROUND_CLASS_NAME,
            w!("OCR Anywhere Background Host"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND::default(),
            None,
            hinstance,
            Some(state_ptr as *const _),
        )
        .is_err()
        {
            show_startup_error(
                lang_for_errors,
                &windows::core::Error::from_win32().to_string(),
            );
            drop(Box::from_raw(state_ptr));
            CoUninitialize();
            return;
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            if ocr_anywhere::ui::handle_dialog_message(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        CoUninitialize();
        let _ = CloseHandle(mutex);
    }
}

/// Reports a failure that prevents the app from starting.
fn show_startup_error(lang: Language, details: &str) {
    task_dialog_with_details(
        "OCR Anywhere",
        lang.startup_failed(),
        lang.capture_failed_content(),
        details,
        &[],
        0,
        DialogIcon::Error,
    );
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        let create_struct = lparam.0 as *const CREATESTRUCTW;
        if let Some(cs) = create_struct.as_ref() {
            let state_ptr = cs.lpCreateParams as *mut MainAppState;
            ocr_anywhere::set_window_userdata(hwnd, state_ptr);
            set_host_window(hwnd);

            let state = &mut *state_ptr;
            state.hotkey_mgr = HotkeyManager::new(hwnd);
            let tooltip = state.language().tray_tooltip(&state.hotkey_label());
            state.tray = Some(TrayIcon::new(hwnd, &tooltip));
            if state
                .coordinator
                .history
                .lock()
                .unwrap()
                .recovered_backup
                .is_some()
            {
                let text = state.language().history_recovered();
                state.notify(text, true);
            }
            state.register_hotkey();
            state.sync_startup_entry();
        }
        return LRESULT(0);
    }

    let state_ptr: *mut MainAppState = ocr_anywhere::get_window_userdata(hwnd);
    // While a capture runs, the overlay's nested message loop also dispatches
    // messages for this window: the state is borrowed by the capture, so leave
    // it alone until the capture finishes.
    if state_ptr.is_null() || capture_in_progress() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *state_ptr;

    if msg == taskbar_created_message() {
        if let Some(tray) = state.tray.as_mut() {
            tray.add();
        }
        return LRESULT(0);
    }

    match msg {
        WM_HOTKEY => {
            if wparam.0 == OCR_HOTKEY_ID as usize {
                state.capture();
            }
            LRESULT(0)
        }
        WM_TRAY_ICON => {
            match (lparam.0 & 0xFFFF) as u32 {
                WM_RBUTTONUP => {
                    let (hotkey, lang) = (state.hotkey_label(), state.language());
                    if let Some(tray) = state.tray.as_ref() {
                        tray.show_menu(&hotkey, lang);
                    }
                }
                WM_LBUTTONDBLCLK => show_history_window(
                    Arc::clone(&state.coordinator.history),
                    Arc::clone(&state.coordinator.settings),
                ),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xFFFF {
                TRAY_MENU_OCR => state.capture(),
                TRAY_MENU_HISTORY => show_history_window(
                    Arc::clone(&state.coordinator.history),
                    Arc::clone(&state.coordinator.settings),
                ),
                TRAY_MENU_SETTINGS => show_settings_window(Arc::clone(&state.coordinator.settings)),
                TRAY_MENU_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_SETTINGS_CHANGED => {
            state.on_settings_changed();
            LRESULT(0)
        }
        WM_DESTROY => {
            ocr_anywhere::clear_window_userdata(hwnd);
            let mut state = Box::from_raw(state_ptr);
            state.hotkey_mgr.unregister();
            state.tray.take();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
