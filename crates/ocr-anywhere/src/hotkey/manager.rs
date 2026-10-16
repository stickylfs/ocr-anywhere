use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};

use crate::settings::storage::HotkeyConfig;

pub const OCR_HOTKEY_ID: i32 = 1001;

pub struct HotkeyManager {
    hwnd: HWND,
    current_config: Option<HotkeyConfig>,
    is_registered: bool,
}

impl HotkeyManager {
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            current_config: None,
            is_registered: false,
        }
    }

    pub fn register(&mut self, config: &HotkeyConfig) -> Result<(), String> {
        self.unregister();

        let modifiers = HOT_KEY_MODIFIERS(config.modifiers | MOD_NOREPEAT.0);
        let vk = config.vk_code;

        unsafe {
            let res = RegisterHotKey(self.hwnd, OCR_HOTKEY_ID, modifiers, vk);
            if res.is_err() {
                return Err(format!(
                    "Hotkey {} is already in use by another application",
                    config.display_string()
                ));
            }
        }

        self.current_config = Some(config.clone());
        self.is_registered = true;
        Ok(())
    }

    pub fn unregister(&mut self) {
        if self.is_registered {
            unsafe {
                let _ = UnregisterHotKey(self.hwnd, OCR_HOTKEY_ID);
            }
            self.is_registered = false;
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.unregister();
    }
}
