use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, HBRUSH};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
};

use crate::settings::storage::ThemeMode;

const DWMWA_USE_IMMERSIVE_DARK_MODE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(20);

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub is_dark: bool,
    pub bg_color: COLORREF,
    pub text_color: COLORREF,
    pub card_bg: COLORREF,
    pub border_color: COLORREF,
    pub accent_color: COLORREF,
    pub input_bg: COLORREF,
    pub secondary_text: COLORREF,
}

impl ThemeColors {
    pub fn light() -> Self {
        Self {
            is_dark: false,
            bg_color: COLORREF(0x00F3F3F3), // #F3F3F3 BGR
            text_color: COLORREF(0x001A1A1A),
            card_bg: COLORREF(0x00FFFFFF),
            border_color: COLORREF(0x00D6D6D6),
            accent_color: COLORREF(0x00D97706),
            input_bg: COLORREF(0x00FFFFFF),
            secondary_text: COLORREF(0x00737373),
        }
    }

    pub fn dark() -> Self {
        Self {
            is_dark: true,
            bg_color: COLORREF(0x00202020), // #202020 BGR
            text_color: COLORREF(0x00F0F0F0),
            card_bg: COLORREF(0x002C2C2C),
            border_color: COLORREF(0x003D3D3D),
            accent_color: COLORREF(0x00FF9E0B),
            input_bg: COLORREF(0x00333333),
            secondary_text: COLORREF(0x009E9E9E),
        }
    }

    pub fn create_bg_brush(&self) -> HBRUSH {
        unsafe { CreateSolidBrush(self.bg_color) }
    }

    pub fn create_card_brush(&self) -> HBRUSH {
        unsafe { CreateSolidBrush(self.card_bg) }
    }
}

pub fn is_system_dark_mode() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey).is_ok() {
            let mut val_type = REG_DWORD;
            let mut data: u32 = 1;
            let mut data_size: u32 = 4;
            let name = w!("AppsUseLightTheme");
            let res = RegQueryValueExW(
                hkey,
                name,
                None,
                Some(&mut val_type),
                Some(&mut data as *mut u32 as *mut u8),
                Some(&mut data_size),
            );
            let _ = RegCloseKey(hkey);
            if res.is_ok() {
                return data == 0;
            }
        }
    }
    false
}

pub fn get_effective_theme(mode: ThemeMode) -> ThemeColors {
    match mode {
        ThemeMode::System => {
            if is_system_dark_mode() {
                ThemeColors::dark()
            } else {
                ThemeColors::light()
            }
        }
        ThemeMode::Dark => ThemeColors::dark(),
        ThemeMode::Light => ThemeColors::light(),
    }
}

pub fn apply_theme_to_window(hwnd: HWND, is_dark: bool) {
    unsafe {
        let val: i32 = if is_dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &val as *const _ as *const std::ffi::c_void,
            4,
        );
    }
}
