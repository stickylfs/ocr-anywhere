pub mod app;
pub mod capture;
pub mod clipboard;
pub mod history;
pub mod hotkey;
pub mod ocr;
pub mod overlay;
pub mod settings;
pub mod tray;
pub mod ui;

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWLP_USERDATA, WM_APP,
};

/// Posted to the background host window after the settings were saved, so it
/// can re-register the hotkey and refresh the tray icon.
pub const WM_APP_SETTINGS_CHANGED: u32 = WM_APP + 1;

/// Background host window (owner of the tray icon, hotkey and clipboard data).
static HOST_WINDOW: AtomicIsize = AtomicIsize::new(0);

pub fn set_host_window(hwnd: HWND) {
    HOST_WINDOW.store(hwnd.0 as isize, Ordering::Relaxed);
}

pub fn host_window() -> HWND {
    HWND(HOST_WINDOW.load(Ordering::Relaxed) as *mut _)
}

/// Encodes a string as a NUL-terminated UTF-16 buffer for Win32 APIs.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Associates a user pointer with a window (works on 32-bit and 64-bit builds).
///
/// # Safety
/// `hwnd` must be a valid window handle and `ptr` must stay valid while stored.
#[inline]
pub unsafe fn set_window_userdata<T>(hwnd: HWND, ptr: *mut T) {
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as _);
}

/// Returns the user pointer stored with [`set_window_userdata`].
///
/// # Safety
/// `hwnd` must be a valid window handle whose user data was set to a `*mut T`.
#[inline]
pub unsafe fn get_window_userdata<T>(hwnd: HWND) -> *mut T {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut T
}

/// Clears the user pointer associated with a window.
///
/// # Safety
/// `hwnd` must be a valid window handle.
#[inline]
pub unsafe fn clear_window_userdata(hwnd: HWND) {
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
}
