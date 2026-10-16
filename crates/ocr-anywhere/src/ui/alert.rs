//! Non-blocking alerts for problems the user has to act on.
//!
//! Each alert is a TaskDialog shown on its own thread: the tray message loop
//! keeps running, and window procedures are never re-entered by the dialog's
//! nested message loop.

use super::dialog::{task_dialog_with_details, Button, DialogIcon};
use crate::tray::TRAY_MENU_SETTINGS;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_COMMAND};

const BUTTON_ACTION: i32 = 100;
const BUTTON_CLOSE: i32 = 101;

/// At most one alert at a time: repeated failures must not stack dialogs.
static ALERT_OPEN: AtomicBool = AtomicBool::new(false);

/// What the optional action button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertAction {
    /// Opens the Settings window of the app.
    OpenSettings,
    /// Opens a URI such as `ms-settings:regionlanguage`.
    Open(&'static str),
}

pub struct Alert {
    pub instruction: String,
    pub content: String,
    /// Technical details, in a collapsible section (hidden when empty).
    pub details: String,
    pub icon: DialogIcon,
    pub action: Option<(String, AlertAction)>,
    pub close_label: String,
}

/// Shows `alert` without blocking the caller. Returns `false` (and shows
/// nothing) when another alert is already open.
pub fn show_alert(alert: Alert) -> bool {
    if ALERT_OPEN.swap(true, Ordering::AcqRel) {
        return false;
    }
    // HWND is not `Send`: pass the host window as an integer.
    let host = crate::host_window().0 as isize;
    std::thread::spawn(move || {
        let mut buttons = Vec::new();
        if let Some((label, _)) = &alert.action {
            buttons.push(Button {
                id: BUTTON_ACTION,
                text: label,
            });
        }
        buttons.push(Button {
            id: BUTTON_CLOSE,
            text: &alert.close_label,
        });

        let choice = task_dialog_with_details(
            "OCR Anywhere",
            &alert.instruction,
            &alert.content,
            &alert.details,
            &buttons,
            if alert.action.is_some() {
                BUTTON_ACTION
            } else {
                BUTTON_CLOSE
            },
            alert.icon,
        );
        ALERT_OPEN.store(false, Ordering::Release);

        if choice != BUTTON_ACTION {
            return;
        }
        match alert.action.map(|(_, action)| action) {
            Some(AlertAction::OpenSettings) => unsafe {
                let _ = PostMessageW(
                    HWND(host as _),
                    WM_COMMAND,
                    WPARAM(TRAY_MENU_SETTINGS),
                    LPARAM(0),
                );
            },
            Some(AlertAction::Open(uri)) => unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let uri = crate::wide(uri);
                ShellExecuteW(
                    None,
                    w!("open"),
                    PCWSTR(uri.as_ptr()),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            },
            None => {}
        }
    });
    true
}
