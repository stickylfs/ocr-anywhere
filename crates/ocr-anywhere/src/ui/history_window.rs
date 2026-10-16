use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, FillRect, SetBkColor, SetBkMode, SetTextColor, HBRUSH, HDC, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowTextLengthW,
    GetWindowTextW, IsWindow, LoadCursorW, LoadIconW, MessageBoxW, RegisterClassExW, SendMessageW,
    SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, CREATESTRUCTW, CW_USEDEFAULT,
    ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, IDCANCEL, IDC_ARROW, IDOK, IDYES,
    LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, LB_ADDSTRING, LB_GETCURSEL, LB_RESETCONTENT, LB_SETCURSEL,
    MB_ICONWARNING, MB_YESNO, MINMAXINFO, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WA_INACTIVE,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN,
    WM_CTLCOLORDLG, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_SIZE, WNDCLASSEXW, WS_CLIPCHILDREN,
    WS_EX_CLIENTEDGE, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VSCROLL,
};

use crate::clipboard::set_clipboard_text;
use crate::history::storage::{HistoryEntry, HistoryStore};
use crate::settings::storage::AppSettings;
use crate::ui::controls::{system_dpi, window_size_for_client, Layout};
use crate::ui::theme::{apply_theme_to_window, get_effective_theme, ThemeColors};
use crate::wide;

const HISTORY_CLASS_NAME: PCWSTR = w!("OCRAnywhere_HistoryClass");

const IDC_SEARCH: usize = 3101;
const IDC_LIST: usize = 3102;
const IDC_PREVIEW: usize = 3103;
const IDC_COPY: usize = 3104;
const IDC_DELETE: usize = 3105;
const IDC_CLEAR_ALL: usize = 3106;

const EN_CHANGE: u16 = 0x0300;
const LBN_SELCHANGE: u16 = 1;
const LBN_DBLCLK: u16 = 2;
const EM_SETCUEBANNER: u32 = 0x1501;

/// Characters of the first line shown per list entry.
const SNIPPET_CHARS: usize = 40;

// Layout in logical (96 DPI) pixels.
const MARGIN: i32 = 16;
const ROW_H: i32 = 26;
const BUTTON_W: i32 = 120;
const INITIAL_CLIENT: (i32, i32) = (760, 480);
const MIN_CLIENT: (i32, i32) = (520, 320);

static OPEN_HISTORY_HWND: AtomicIsize = AtomicIsize::new(0);

/// The history window, if open.
pub fn open_window() -> Option<HWND> {
    let hwnd = HWND(OPEN_HISTORY_HWND.load(Ordering::Relaxed) as *mut _);
    (!hwnd.0.is_null() && unsafe { IsWindow(hwnd).as_bool() }).then_some(hwnd)
}

struct Controls {
    search: HWND,
    clear: HWND,
    list: HWND,
    preview: HWND,
    copy: HWND,
    delete: HWND,
}

struct HistoryWindowState {
    store: Arc<Mutex<HistoryStore>>,
    settings: Arc<Mutex<AppSettings>>,
    lang: ocr_anywhere_common::Language,
    colors: ThemeColors,
    bg_brush: HBRUSH,
    card_brush: HBRUSH,
    layout: Option<Layout>,
    controls: Option<Controls>,
    filtered_entries: Vec<HistoryEntry>,
}

impl Drop for HistoryWindowState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.bg_brush);
            let _ = DeleteObject(self.card_brush);
        }
    }
}

/// Opens the history window (or focuses it when already open).
#[allow(clippy::manual_dangling_ptr)]
pub fn show_history_window(store: Arc<Mutex<HistoryStore>>, settings: Arc<Mutex<AppSettings>>) {
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
        let lang = ocr_anywhere_common::Language::resolve(lang_mode);
        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let app_icon = LoadIconW(hinstance, PCWSTR(1 as *const u16)).unwrap_or_default();

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(history_wnd_proc),
            hInstance: hinstance.into(),
            hIcon: app_icon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: HISTORY_CLASS_NAME,
            hIconSm: app_icon,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wnd_class);

        let state_ptr = Box::into_raw(Box::new(HistoryWindowState {
            store,
            settings,
            lang,
            colors,
            bg_brush: colors.create_bg_brush(),
            card_brush: colors.create_card_brush(),
            layout: None,
            controls: None,
            filtered_entries: Vec::new(),
        }));

        let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
        let ex_style = WS_EX_CONTROLPARENT;
        let (width, height) = window_size_for_client(
            INITIAL_CLIENT.0,
            INITIAL_CLIENT.1,
            style,
            ex_style,
            system_dpi(),
        );
        let title = wide(lang.history_title());

        match CreateWindowExW(
            ex_style,
            HISTORY_CLASS_NAME,
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
                OPEN_HISTORY_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
                apply_theme_to_window(hwnd, colors.is_dark);
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
            }
            Err(_) => drop(Box::from_raw(state_ptr)),
        }
    }
}

/// Formats a Unix timestamp in the local time zone as `YYYY-MM-DD HH:MM`.
fn format_timestamp(timestamp_secs: u64) -> String {
    // FILETIME counts 100 ns intervals since 1601-01-01.
    let ticks = (timestamp_secs + 11_644_473_600) * 10_000_000;
    let file_time = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        if FileTimeToSystemTime(&file_time, &mut utc).is_err()
            || SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err()
        {
            return String::new();
        }
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute
    )
}

fn list_label(entry: &HistoryEntry) -> String {
    let first_line = entry.text.lines().next().unwrap_or("").trim();
    let mut snippet: String = first_line.chars().take(SNIPPET_CHARS).collect();
    if first_line.chars().count() > SNIPPET_CHARS {
        snippet.push('…');
    }
    format!("{}  {}", format_timestamp(entry.timestamp), snippet)
}

unsafe fn window_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let copied = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
    String::from_utf16_lossy(&buf[..copied])
}

unsafe fn selected_index(state: &HistoryWindowState) -> Option<usize> {
    let c = state.controls.as_ref()?;
    let index = SendMessageW(c.list, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    usize::try_from(index)
        .ok()
        .filter(|&i| i < state.filtered_entries.len())
}

/// Re-reads the store, applies the search filter and refills the list,
/// keeping the selected entry when it still exists.
unsafe fn refresh(state: &mut HistoryWindowState) {
    let Some(c) = &state.controls else {
        return;
    };
    let previous_id = selected_index(state).map(|i| state.filtered_entries[i].id.clone());
    let query = window_text(c.search).to_lowercase();
    let query = query.trim();

    let all = state.store.lock().unwrap().entries.clone();
    state.filtered_entries = if query.is_empty() {
        all
    } else {
        all.into_iter()
            .filter(|e| e.text.to_lowercase().contains(query))
            .collect()
    };

    let c = state.controls.as_ref().unwrap();
    SendMessageW(c.list, LB_RESETCONTENT, WPARAM(0), LPARAM(0));
    for entry in &state.filtered_entries {
        let label = wide(&list_label(entry));
        SendMessageW(
            c.list,
            LB_ADDSTRING,
            WPARAM(0),
            LPARAM(label.as_ptr() as isize),
        );
    }

    let selection = previous_id
        .and_then(|id| state.filtered_entries.iter().position(|e| e.id == id))
        .or((!state.filtered_entries.is_empty()).then_some(0));
    if let Some(i) = selection {
        SendMessageW(c.list, LB_SETCURSEL, WPARAM(i), LPARAM(0));
    }
    update_preview(state);
}

unsafe fn update_preview(state: &HistoryWindowState) {
    let Some(c) = &state.controls else {
        return;
    };
    let text = match selected_index(state) {
        Some(i) => state.filtered_entries[i].text.clone(),
        None if !state.settings.lock().unwrap().history_enabled => {
            state.lang.history_disabled().to_string()
        }
        None => state.lang.history_no_items().to_string(),
    };
    // The multiline EDIT control needs CRLF line breaks.
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let text_w = wide(&text);
    let _ = SetWindowTextW(c.preview, PCWSTR(text_w.as_ptr()));
}

unsafe fn build_controls(hwnd: HWND, state: &mut HistoryWindowState) {
    let lang = state.lang;
    let mut layout = Layout::new(hwnd, state.colors.is_dark);
    let zero = (0, 0, 10, 10); // positioned by `arrange`

    let search = layout.edit(IDC_SEARCH, WS_TABSTOP, WS_EX_CLIENTEDGE, zero);
    let cue = wide(lang.history_search_placeholder());
    SendMessageW(
        search,
        EM_SETCUEBANNER,
        WPARAM(1),
        LPARAM(cue.as_ptr() as isize),
    );

    let clear = layout.button(IDC_CLEAR_ALL, lang.history_clear_all(), false, zero);
    let list = layout.list_box(
        IDC_LIST,
        WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE((LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32),
        WS_EX_CLIENTEDGE,
        zero,
    );
    let preview = layout.edit(
        IDC_PREVIEW,
        WS_TABSTOP
            | WS_VSCROLL
            | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
        WS_EX_CLIENTEDGE,
        zero,
    );
    let copy = layout.button(IDC_COPY, lang.history_copy(), true, zero);
    let delete = layout.button(IDC_DELETE, lang.history_delete(), false, zero);

    state.layout = Some(layout);
    state.controls = Some(Controls {
        search,
        clear,
        list,
        preview,
        copy,
        delete,
    });
    arrange(hwnd, state);
    refresh(state);
}

/// Lays out the controls for the current client size.
unsafe fn arrange(hwnd: HWND, state: &mut HistoryWindowState) {
    let (Some(layout), Some(c)) = (state.layout.as_mut(), state.controls.as_ref()) else {
        return;
    };
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let width = layout.to_logical(rect.right - rect.left);
    let height = layout.to_logical(rect.bottom - rect.top);

    let list_w = ((width - MARGIN * 3) * 2 / 5).max(200);
    let right_x = MARGIN * 2 + list_w;
    let right_w = (width - right_x - MARGIN).max(100);
    let body_y = MARGIN + ROW_H + 8;
    let body_h = (height - body_y - MARGIN).max(60);
    let buttons_y = height - MARGIN - ROW_H - 2;

    layout.place(c.search, (MARGIN, MARGIN, list_w, ROW_H));
    layout.place(
        c.clear,
        (width - MARGIN - BUTTON_W, MARGIN, BUTTON_W, ROW_H + 2),
    );
    layout.place(c.list, (MARGIN, body_y, list_w, body_h));
    layout.place(
        c.preview,
        (right_x, body_y, right_w, (buttons_y - body_y - 8).max(40)),
    );
    layout.place(c.copy, (right_x, buttons_y, BUTTON_W, ROW_H + 2));
    layout.place(
        c.delete,
        (right_x + BUTTON_W + 8, buttons_y, BUTTON_W, ROW_H + 2),
    );
}

unsafe fn copy_selected(state: &HistoryWindowState) {
    if let Some(i) = selected_index(state) {
        let _ = set_clipboard_text(&state.filtered_entries[i].text);
    }
}

unsafe extern "system" fn history_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        if let Some(cs) = (lparam.0 as *const CREATESTRUCTW).as_ref() {
            let state_ptr = cs.lpCreateParams as *mut HistoryWindowState;
            crate::set_window_userdata(hwnd, state_ptr);
            build_controls(hwnd, &mut *state_ptr);
        }
        return LRESULT(0);
    }

    let state_ptr: *mut HistoryWindowState = crate::get_window_userdata(hwnd);
    let Some(state) = state_ptr.as_mut() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };

    match msg {
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u16;
            match id {
                IDC_SEARCH if notification == EN_CHANGE => refresh(state),
                IDC_LIST if notification == LBN_SELCHANGE => update_preview(state),
                IDC_LIST if notification == LBN_DBLCLK => copy_selected(state),
                IDC_COPY => copy_selected(state),
                _ if id == IDOK.0 as usize => copy_selected(state),
                IDC_DELETE => {
                    if let Some(i) = selected_index(state) {
                        let id = state.filtered_entries[i].id.clone();
                        state.store.lock().unwrap().delete_entry(&id);
                        refresh(state);
                    }
                }
                IDC_CLEAR_ALL => {
                    let text = wide(state.lang.history_clear_confirm());
                    let title = wide(state.lang.history_title());
                    let answer = MessageBoxW(
                        hwnd,
                        PCWSTR(text.as_ptr()),
                        PCWSTR(title.as_ptr()),
                        MB_YESNO | MB_ICONWARNING,
                    );
                    if answer == IDYES {
                        let _ = state.store.lock().unwrap().clear();
                        refresh(state);
                    }
                }
                _ if id == IDCANCEL.0 as usize => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // Pick up captures made while the window was open in the background.
            if (wparam.0 & 0xFFFF) as u32 != WA_INACTIVE {
                refresh(state);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SIZE => {
            arrange(hwnd, state);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            if let (Some(info), Some(layout)) = (
                (lparam.0 as *mut MINMAXINFO).as_mut(),
                state.layout.as_ref(),
            ) {
                let (w, h) = window_size_for_client(
                    MIN_CLIENT.0,
                    MIN_CLIENT.1,
                    WS_OVERLAPPEDWINDOW,
                    WINDOW_EX_STYLE(0),
                    layout.dpi(),
                );
                info.ptMinTrackSize.x = w;
                info.ptMinTrackSize.y = h;
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(layout) = state.layout.as_mut() {
                layout.apply_dpi((wparam.0 & 0xFFFF) as u32);
            }
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
            arrange(hwnd, state);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
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
            OPEN_HISTORY_HWND.store(0, Ordering::Relaxed);
            crate::clear_window_userdata(hwnd);
            drop(Box::from_raw(state_ptr));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_label_truncates_first_line() {
        let entry = HistoryEntry {
            id: "1".to_string(),
            timestamp: 0,
            text: format!("{}\r\nsecond line", "x".repeat(60)),
        };
        let label = list_label(&entry);
        assert!(label.ends_with('…'));
        assert!(!label.contains("second"));
    }

    #[test]
    fn test_timestamp_format() {
        // 2026-01-15 12:00 UTC; the local rendering depends on the time zone.
        let text = format_timestamp(1_768_478_400);
        assert_eq!(text.len(), 16);
        assert!(text.starts_with("2026-01-1"));
    }
}
