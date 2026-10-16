use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, NOTIFY_ICON_INFOTIP_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, LoadIconW,
    LoadImageW, PostMessageW, RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu, HICON,
    IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LR_SHARED, MF_SEPARATOR, MF_STRING, SM_CXSMICON,
    SM_CYSMICON, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_NULL, WM_USER,
};

use crate::wide;

pub const WM_TRAY_ICON: u32 = WM_USER + 100;

pub const TRAY_MENU_OCR: usize = 2001;
pub const TRAY_MENU_HISTORY: usize = 2002;
pub const TRAY_MENU_SETTINGS: usize = 2003;
pub const TRAY_MENU_EXIT: usize = 2004;

/// Message broadcast by Explorer when the taskbar is (re)created, e.g. after
/// `explorer.exe` restarts; tray icons must then be added again.
pub fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

/// Copies `text` into a fixed-size UTF-16 buffer, truncating if needed.
fn fill<const N: usize>(text: &str) -> [u16; N] {
    let mut buf = [0u16; N];
    for (dst, src) in buf.iter_mut().zip(text.encode_utf16().take(N - 1)) {
        *dst = src;
    }
    buf
}

pub struct TrayIcon {
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
    is_added: bool,
}

impl TrayIcon {
    #[allow(clippy::manual_dangling_ptr)]
    pub fn new(hwnd: HWND, tooltip: &str) -> Self {
        let hicon = unsafe {
            let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
            LoadImageW(
                hinstance,
                PCWSTR(1 as *const u16),
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR | LR_SHARED,
            )
            .map(|h| HICON(h.0))
            .or_else(|_| LoadIconW(hinstance, PCWSTR(1 as *const u16)))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
        };

        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY_ICON,
            hIcon: hicon,
            szTip: fill(tooltip),
            ..Default::default()
        };

        let mut tray = Self {
            hwnd,
            nid,
            is_added: false,
        };
        tray.add();
        tray
    }

    /// Adds the icon to the notification area (again after an Explorer restart).
    pub fn add(&mut self) {
        self.nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        self.is_added = unsafe { Shell_NotifyIconW(NIM_ADD, &self.nid) }.as_bool();
    }

    pub fn remove(&mut self) {
        if self.is_added {
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
            }
            self.is_added = false;
        }
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        self.nid.szTip = fill(tooltip);
        self.nid.uFlags = NIF_TIP;
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &self.nid);
        }
    }

    /// Shows a balloon / toast notification anchored to the tray icon.
    pub fn notify(&mut self, title: &str, text: &str, warning: bool) {
        let mut nid = self.nid;
        nid.uFlags = NIF_INFO;
        nid.szInfoTitle = fill(title);
        nid.szInfo = fill(text);
        nid.dwInfoFlags = if warning {
            NIIF_WARNING
        } else {
            NOTIFY_ICON_INFOTIP_FLAGS(NIIF_INFO.0)
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn show_menu(&self, hotkey_label: &str, lang: ocr_anywhere_common::Language) {
        unsafe {
            let Ok(hmenu) = CreatePopupMenu() else {
                return;
            };

            let items = [
                (TRAY_MENU_OCR, lang.tray_extract_text(hotkey_label)),
                (TRAY_MENU_HISTORY, lang.tray_history().to_string()),
                (TRAY_MENU_SETTINGS, lang.tray_settings().to_string()),
            ];
            for (id, label) in &items {
                let label_w = wide(label);
                let _ = AppendMenuW(hmenu, MF_STRING, *id, PCWSTR(label_w.as_ptr()));
            }
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let exit_w = wide(lang.tray_exit());
            let _ = AppendMenuW(hmenu, MF_STRING, TRAY_MENU_EXIT, PCWSTR(exit_w.as_ptr()));

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            // Recommended focus handling for tray popup menus (KB135788).
            let _ = SetForegroundWindow(self.hwnd);
            let _ = TrackPopupMenu(
                hmenu,
                TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(hmenu);
            let _ = PostMessageW(self.hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
    }
}
