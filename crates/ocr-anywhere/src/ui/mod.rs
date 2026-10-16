pub mod alert;
pub mod controls;
pub mod dialog;
pub mod history_window;
pub mod settings_window;
pub mod theme;

pub use history_window::show_history_window;
pub use settings_window::show_settings_window;
pub use theme::{apply_theme_to_window, get_effective_theme, is_system_dark_mode, ThemeColors};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsDialogMessageW, MSG};

/// Routes keyboard navigation (Tab, Enter, Esc) to the open settings or history
/// window. Returns `true` when the message was handled and must not be
/// dispatched again.
pub fn handle_dialog_message(msg: &MSG) -> bool {
    [
        settings_window::open_window(),
        history_window::open_window(),
    ]
    .into_iter()
    .flatten()
    .any(|hwnd: HWND| unsafe { IsDialogMessageW(hwnd, msg).as_bool() })
}
