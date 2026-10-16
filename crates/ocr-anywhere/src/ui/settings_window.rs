use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, FillRect, SetBkColor, SetBkMode, SetTextColor, HBRUSH, HDC, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, LoadCursorW, LoadIconW, PostMessageW,
    RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow, CREATESTRUCTW, CW_USEDEFAULT,
    IDCANCEL, IDC_ARROW, IDOK, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WM_COMMAND, WM_CREATE,
    WM_CTLCOLORBTN, WM_CTLCOLORDLG, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WNDCLASSEXW, WS_CAPTION, WS_CLIPCHILDREN,
    WS_EX_CONTROLPARENT, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

use crate::capture::monitor::get_all_monitors;
use crate::ocr::language::OcrManager;
use crate::settings::storage::{
    AppSettings, HotkeyConfig, Language, LanguageMode, MonitorTarget, ThemeMode,
};
use crate::ui::alert::{show_alert, Alert};
use crate::ui::controls::{
    combo_select, combo_selected, is_checked, set_checked, system_dpi, window_size_for_client,
    Layout,
};
use crate::ui::dialog::DialogIcon;
use crate::ui::theme::{apply_theme_to_window, get_effective_theme, ThemeColors};
use crate::{wide, WM_APP_SETTINGS_CHANGED};

const SETTINGS_CLASS_NAME: PCWSTR = w!("OCRAnywhereSettingsWindow");

const IDC_CB_UI_LANG: usize = 3001;
const IDC_CB_THEME: usize = 3002;
const IDC_CB_MONITOR: usize = 3003;
const IDC_CB_MODIFIERS: usize = 3004;
const IDC_CB_KEY: usize = 3005;
const IDC_CHK_STARTUP: usize = 3006;
const IDC_CB_OCR_LANG: usize = 3007;
const IDC_CHK_HISTORY: usize = 3008;
const IDC_CB_MAX_ENTRIES: usize = 3009;
const IDC_BTN_SAVE: usize = 3010;
const IDC_BTN_CANCEL: usize = 3011;

// Layout in logical (96 DPI) pixels.
const CLIENT_WIDTH: i32 = 480;
const CLIENT_HEIGHT: i32 = 466;
const LABEL_X: i32 = 24;
const LABEL_W: i32 = 176;
const FIELD_X: i32 = 208;
const FIELD_W: i32 = 248;
const ROW_H: i32 = 24;

const MAX_ENTRY_CHOICES: [usize; 5] = [50, 100, 250, 500, 1000];

static OPEN_SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);

/// The settings window, if open.
pub fn open_window() -> Option<HWND> {
    let hwnd = HWND(OPEN_SETTINGS_HWND.load(Ordering::Relaxed) as *mut _);
    (!hwnd.0.is_null() && unsafe { IsWindow(hwnd).as_bool() }).then_some(hwnd)
}

/// Modifier combinations offered in the hotkey picker.
fn modifier_choices() -> Vec<u32> {
    let (ctrl, alt, shift, win) = (MOD_CONTROL.0, MOD_ALT.0, MOD_SHIFT.0, MOD_WIN.0);
    vec![
        ctrl | alt,
        ctrl | shift,
        alt | shift,
        ctrl | alt | shift,
        win | shift,
        win | alt,
        ctrl | win,
    ]
}

/// Keys offered in the hotkey picker: A-Z, 0-9, F1-F12.
fn key_choices() -> Vec<u32> {
    (0x41..=0x5A)
        .chain(0x30..=0x39)
        .chain(0x70..=0x7B)
        .collect()
}

/// Returns the index of `current` in `values`, appending it when missing so a
/// value configured in `config.ini` is never silently replaced.
fn index_or_append<T: PartialEq>(values: &mut Vec<T>, current: T) -> usize {
    match values.iter().position(|v| *v == current) {
        Some(i) => i,
        None => {
            values.push(current);
            values.len() - 1
        }
    }
}

struct Controls {
    ui_lang: HWND,
    theme: HWND,
    monitor: HWND,
    modifiers: HWND,
    key: HWND,
    startup: HWND,
    ocr_lang: HWND,
    history: HWND,
    max_entries: HWND,
}

struct SettingsWindowState {
    settings: Arc<Mutex<AppSettings>>,
    lang: Language,
    colors: ThemeColors,
    bg_brush: HBRUSH,
    card_brush: HBRUSH,
    layout: Option<Layout>,
    controls: Option<Controls>,
    monitor_values: Vec<MonitorTarget>,
    modifier_values: Vec<u32>,
    key_values: Vec<u32>,
    ocr_values: Vec<String>,
    max_entries_values: Vec<usize>,
}

impl Drop for SettingsWindowState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.bg_brush);
            let _ = DeleteObject(self.card_brush);
        }
    }
}

/// Opens the settings window (or focuses it when already open). Saved changes
/// are applied by the host window via `WM_APP_SETTINGS_CHANGED`.
#[allow(clippy::manual_dangling_ptr)]
pub fn show_settings_window(settings: Arc<Mutex<AppSettings>>) {
    unsafe {
        if let Some(hwnd) = open_window() {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            return;
        }

        let (theme_mode, lang_mode) = {
            let s = settings.lock().unwrap();
            (s.theme, s.language)
        };
        let colors = get_effective_theme(theme_mode);
        let lang = Language::resolve(lang_mode);
        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let app_icon = LoadIconW(hinstance, PCWSTR(1 as *const u16)).unwrap_or_default();
        let bg_brush = colors.create_bg_brush();

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(settings_wnd_proc),
            hInstance: hinstance.into(),
            hIcon: app_icon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: SETTINGS_CLASS_NAME,
            hIconSm: app_icon,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wnd_class);

        let state_ptr = Box::into_raw(Box::new(SettingsWindowState {
            settings,
            lang,
            colors,
            bg_brush,
            card_brush: colors.create_card_brush(),
            layout: None,
            controls: None,
            monitor_values: Vec::new(),
            modifier_values: Vec::new(),
            key_values: Vec::new(),
            ocr_values: Vec::new(),
            max_entries_values: Vec::new(),
        }));

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let ex_style = WS_EX_CONTROLPARENT;
        let (width, height) =
            window_size_for_client(CLIENT_WIDTH, CLIENT_HEIGHT, style, ex_style, system_dpi());
        let title = wide(lang.settings_title());

        match CreateWindowExW(
            ex_style,
            SETTINGS_CLASS_NAME,
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            HWND::default(),
            None,
            hinstance,
            Some(state_ptr as *const _),
        ) {
            Ok(hwnd) => {
                OPEN_SETTINGS_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                apply_theme_to_window(hwnd, colors.is_dark);
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
            }
            Err(_) => drop(Box::from_raw(state_ptr)),
        }
    }
}

unsafe fn build_controls(hwnd: HWND, state: &mut SettingsWindowState) {
    let s = state.settings.lock().unwrap().clone();
    let lang = state.lang;
    let mut layout = Layout::new(hwnd, state.colors.is_dark);

    let row = |layout: &mut Layout, y: i32, label: &str| {
        layout.label(label, (LABEL_X, y + 4, LABEL_W, 20), false);
    };

    // --- General ---
    layout.label(
        lang.settings_group_general(),
        (LABEL_X - 8, 16, 400, 20),
        true,
    );

    row(&mut layout, 44, lang.settings_ui_lang_label());
    let ui_lang = layout.combo(
        IDC_CB_UI_LANG,
        &[
            lang.settings_ui_lang_auto().to_string(),
            "English".to_string(),
            "Español".to_string(),
        ],
        (FIELD_X, 44, FIELD_W, ROW_H),
    );
    combo_select(
        ui_lang,
        match s.language {
            LanguageMode::Auto => 0,
            LanguageMode::En => 1,
            LanguageMode::Es => 2,
        },
    );

    row(&mut layout, 78, lang.settings_theme_label());
    let theme = layout.combo(
        IDC_CB_THEME,
        &[
            lang.settings_theme_system().to_string(),
            lang.settings_theme_light().to_string(),
            lang.settings_theme_dark().to_string(),
        ],
        (FIELD_X, 78, FIELD_W, ROW_H),
    );
    combo_select(
        theme,
        match s.theme {
            ThemeMode::System => 0,
            ThemeMode::Light => 1,
            ThemeMode::Dark => 2,
        },
    );

    row(&mut layout, 112, lang.settings_monitor_label());
    state.monitor_values = vec![MonitorTarget::Cursor, MonitorTarget::Primary];
    state
        .monitor_values
        .extend((0..get_all_monitors().len()).map(MonitorTarget::Index));
    let monitor_selected = index_or_append(&mut state.monitor_values, s.monitor_target.clone());
    let monitor_labels: Vec<String> = state
        .monitor_values
        .iter()
        .map(|m| match m {
            MonitorTarget::Cursor => lang.settings_monitor_cursor().to_string(),
            MonitorTarget::Primary => lang.settings_monitor_primary().to_string(),
            MonitorTarget::Index(i) => lang.settings_monitor_index(*i),
        })
        .collect();
    let monitor = layout.combo(
        IDC_CB_MONITOR,
        &monitor_labels,
        (FIELD_X, 112, FIELD_W, ROW_H),
    );
    combo_select(monitor, monitor_selected);

    row(&mut layout, 146, lang.settings_hotkey_label());
    state.modifier_values = modifier_choices();
    let modifier_selected = index_or_append(&mut state.modifier_values, s.hotkey.modifiers);
    let modifier_labels: Vec<String> = state
        .modifier_values
        .iter()
        .map(|&m| {
            let label = HotkeyConfig {
                modifiers: m,
                vk_code: 0x41,
            }
            .display_string();
            label.trim_end_matches("+A").to_string()
        })
        .collect();
    let modifiers = layout.combo(
        IDC_CB_MODIFIERS,
        &modifier_labels,
        (FIELD_X, 146, 136, ROW_H),
    );
    combo_select(modifiers, modifier_selected);

    state.key_values = key_choices();
    let key_selected = index_or_append(&mut state.key_values, s.hotkey.vk_code);
    let key_labels: Vec<String> = state
        .key_values
        .iter()
        .map(|&vk| {
            HotkeyConfig {
                modifiers: 0,
                vk_code: vk,
            }
            .key_to_string()
        })
        .collect();
    let key = layout.combo(
        IDC_CB_KEY,
        &key_labels,
        (FIELD_X + 144, 146, FIELD_W - 144, ROW_H),
    );
    combo_select(key, key_selected);

    let startup = layout.checkbox(
        IDC_CHK_STARTUP,
        lang.settings_startup_label(),
        (LABEL_X, 182, 432, 22),
    );
    set_checked(startup, s.start_with_windows);

    // --- Text recognition ---
    layout.label(lang.settings_group_ocr(), (LABEL_X - 8, 218, 400, 20), true);
    row(&mut layout, 246, lang.settings_ocr_lang_label());
    state.ocr_values = vec!["auto".to_string()];
    state
        .ocr_values
        .extend(OcrManager::get_available_languages());
    let ocr_selected = if s.ocr_language.eq_ignore_ascii_case("auto") {
        0
    } else {
        index_or_append(&mut state.ocr_values, s.ocr_language.clone())
    };
    let mut ocr_labels = state.ocr_values.clone();
    ocr_labels[0] = lang.settings_ocr_lang_auto().to_string();
    let ocr_lang = layout.combo(IDC_CB_OCR_LANG, &ocr_labels, (FIELD_X, 246, FIELD_W, ROW_H));
    combo_select(ocr_lang, ocr_selected);

    // --- History ---
    layout.label(
        lang.settings_group_history(),
        (LABEL_X - 8, 282, 400, 20),
        true,
    );
    let history = layout.checkbox(
        IDC_CHK_HISTORY,
        lang.settings_history_label(),
        (LABEL_X, 310, 432, 22),
    );
    set_checked(history, s.history_enabled);

    row(&mut layout, 344, lang.settings_max_history_label());
    state.max_entries_values = MAX_ENTRY_CHOICES.to_vec();
    let max_selected = index_or_append(&mut state.max_entries_values, s.max_history_entries);
    let default_entries = AppSettings::default().max_history_entries;
    let max_labels: Vec<String> = state
        .max_entries_values
        .iter()
        .map(|&n| {
            if n == default_entries {
                format!("{}{}", n, lang.settings_default_suffix())
            } else {
                n.to_string()
            }
        })
        .collect();
    let max_entries = layout.combo(
        IDC_CB_MAX_ENTRIES,
        &max_labels,
        (FIELD_X, 344, FIELD_W, ROW_H),
    );
    combo_select(max_entries, max_selected);
    let _ = EnableWindow(max_entries, s.history_enabled);

    layout.label(
        lang.settings_privacy_badge(),
        (LABEL_X, 388, 432, 20),
        false,
    );

    layout.button(
        IDC_BTN_SAVE,
        lang.settings_save(),
        true,
        (252, 422, 100, 28),
    );
    layout.button(
        IDC_BTN_CANCEL,
        lang.settings_cancel(),
        false,
        (360, 422, 96, 28),
    );

    state.layout = Some(layout);
    state.controls = Some(Controls {
        ui_lang,
        theme,
        monitor,
        modifiers,
        key,
        startup,
        ocr_lang,
        history,
        max_entries,
    });
}

/// Reads the controls into the shared settings, saves them and notifies the host.
unsafe fn save(state: &SettingsWindowState) {
    let Some(c) = &state.controls else {
        return;
    };
    let pick = |hwnd: HWND| combo_selected(hwnd).unwrap_or(0);

    {
        let mut s = state.settings.lock().unwrap();
        s.language = match pick(c.ui_lang) {
            1 => LanguageMode::En,
            2 => LanguageMode::Es,
            _ => LanguageMode::Auto,
        };
        s.theme = match pick(c.theme) {
            1 => ThemeMode::Light,
            2 => ThemeMode::Dark,
            _ => ThemeMode::System,
        };
        if let Some(m) = state.monitor_values.get(pick(c.monitor)) {
            s.monitor_target = m.clone();
        }
        if let (Some(&modifiers), Some(&vk_code)) = (
            state.modifier_values.get(pick(c.modifiers)),
            state.key_values.get(pick(c.key)),
        ) {
            s.hotkey = HotkeyConfig { modifiers, vk_code };
        }
        s.start_with_windows = is_checked(c.startup);
        if let Some(tag) = state.ocr_values.get(pick(c.ocr_lang)) {
            s.ocr_language = tag.clone();
        }
        s.history_enabled = is_checked(c.history);
        if let Some(&n) = state.max_entries_values.get(pick(c.max_entries)) {
            s.max_history_entries = n;
        }
        if let Err(e) = s.save() {
            let lang = Language::resolve(s.language);
            let path = ocr_anywhere_common::AppConfig::get_config_path();
            show_alert(Alert {
                instruction: lang.settings_save_failed().to_string(),
                content: lang.settings_save_failed_content(&path.display().to_string()),
                details: e.to_string(),
                icon: DialogIcon::Error,
                action: None,
                close_label: lang.alert_close().to_string(),
            });
        }
    }

    let _ = PostMessageW(
        crate::host_window(),
        WM_APP_SETTINGS_CHANGED,
        WPARAM(0),
        LPARAM(0),
    );
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        if let Some(cs) = (lparam.0 as *const CREATESTRUCTW).as_ref() {
            let state_ptr = cs.lpCreateParams as *mut SettingsWindowState;
            crate::set_window_userdata(hwnd, state_ptr);
            build_controls(hwnd, &mut *state_ptr);
        }
        return LRESULT(0);
    }

    let state_ptr: *mut SettingsWindowState = crate::get_window_userdata(hwnd);
    let Some(state) = state_ptr.as_mut() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };

    match msg {
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            match id {
                // Enter (IDOK via IsDialogMessage) or the Save button
                IDC_BTN_SAVE => {
                    save(state);
                    let _ = DestroyWindow(hwnd);
                }
                _ if id == IDOK.0 as usize => {
                    save(state);
                    let _ = DestroyWindow(hwnd);
                }
                // Esc (IDCANCEL via IsDialogMessage) or the Cancel button
                IDC_BTN_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                _ if id == IDCANCEL.0 as usize => {
                    let _ = DestroyWindow(hwnd);
                }
                IDC_CHK_HISTORY => {
                    if let Some(c) = &state.controls {
                        let _ = EnableWindow(c.max_entries, is_checked(c.history));
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let suggested = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if let Some(layout) = state.layout.as_mut() {
                layout.apply_dpi((wparam.0 & 0xFFFF) as u32);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let mut rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
            FillRect(HDC(wparam.0 as _), &rect, state.bg_brush);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORDLG | WM_CTLCOLORBTN => {
            let hdc = HDC(wparam.0 as _);
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, state.colors.text_color);
            SetBkColor(hdc, state.colors.bg_color);
            LRESULT(state.bg_brush.0 as isize)
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let hdc = HDC(wparam.0 as _);
            SetBkColor(hdc, state.colors.input_bg);
            SetTextColor(hdc, state.colors.text_color);
            LRESULT(state.card_brush.0 as isize)
        }
        WM_DESTROY => {
            OPEN_SETTINGS_HWND.store(0, Ordering::Relaxed);
            crate::clear_window_userdata(hwnd);
            drop(Box::from_raw(state_ptr));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
