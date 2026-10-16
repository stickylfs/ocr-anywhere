//! Helpers for building DPI-aware Win32 child-control layouts.
//!
//! Controls are positioned in logical (96 DPI) units and scaled to the window's
//! actual DPI; [`Layout::apply_dpi`] re-scales everything when the window moves
//! to a monitor with a different scale factor.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateFontW, DeleteObject, FW_BOLD, FW_NORMAL, HFONT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SendMessageW, SetWindowPos, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL,
    ES_AUTOHSCROLL, HMENU, SWP_NOACTIVATE, SWP_NOZORDER, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT,
    WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use crate::wide;

/// `SS_NOPREFIX`: show `&` literally in static text instead of as a mnemonic.
const SS_NOPREFIX: u32 = 0x80;

/// Height of a dropdown list (the closed combo box height is set by Windows).
const COMBO_DROP_HEIGHT: i32 = 240;

pub fn scale(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi as i64 / 96) as i32
}

pub fn system_dpi() -> u32 {
    unsafe { GetDpiForSystem() }.max(96)
}

pub fn window_dpi(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => system_dpi(),
        dpi => dpi,
    }
}

/// Outer window size for a client area of `width` x `height` logical pixels.
pub fn window_size_for_client(
    width: i32,
    height: i32,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    dpi: u32,
) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: scale(width, dpi),
        bottom: scale(height, dpi),
    };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rect, style, false, ex_style, dpi);
    }
    (rect.right - rect.left, rect.bottom - rect.top)
}

/// Segoe UI 9 pt at `dpi`.
fn create_font(dpi: u32, bold: bool) -> HFONT {
    let height = -(9 * dpi as i32 / 72);
    let weight = if bold { FW_BOLD } else { FW_NORMAL };
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            0,
            0,
            0,
            5, // CLEARTYPE_QUALITY
            0,
            w!("Segoe UI"),
        )
    }
}

struct Child {
    hwnd: HWND,
    /// Logical (96 DPI) bounds.
    bounds: RECT,
    bold: bool,
}

/// Children of one window, with the fonts used for the current DPI.
pub struct Layout {
    parent: HWND,
    dpi: u32,
    font: HFONT,
    font_bold: HFONT,
    dark: bool,
    children: Vec<Child>,
}

impl Drop for Layout {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font);
            let _ = DeleteObject(self.font_bold);
        }
    }
}

impl Layout {
    pub fn new(parent: HWND, dark: bool) -> Self {
        let dpi = window_dpi(parent);
        Self {
            parent,
            dpi,
            font: create_font(dpi, false),
            font_bold: create_font(dpi, true),
            dark,
            children: Vec::new(),
        }
    }

    pub fn dpi(&self) -> u32 {
        self.dpi
    }

    /// Converts physical pixels to logical units.
    pub fn to_logical(&self, value: i32) -> i32 {
        (value as i64 * 96 / self.dpi as i64) as i32
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        ex_style: WINDOW_EX_STYLE,
        id: usize,
        bounds: (i32, i32, i32, i32),
        bold: bool,
    ) -> HWND {
        let (x, y, w, h) = bounds;
        let text_w = wide(text);
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class,
                PCWSTR(text_w.as_ptr()),
                WS_CHILD | WS_VISIBLE | style,
                scale(x, self.dpi),
                scale(y, self.dpi),
                scale(w, self.dpi),
                scale(h, self.dpi),
                self.parent,
                HMENU(id as _),
                GetModuleHandleW(None).unwrap_or_default(),
                None,
            )
            .unwrap_or_default()
        };
        let font = if bold { self.font_bold } else { self.font };
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
        self.children.push(Child {
            hwnd,
            bounds: RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            bold,
        });
        hwnd
    }

    /// Applies the dark variant of the common controls theme when needed.
    fn theme(&self, hwnd: HWND, dark_theme: PCWSTR) {
        if self.dark {
            unsafe {
                let _ = SetWindowTheme(hwnd, dark_theme, PCWSTR::null());
            }
        }
    }

    pub fn label(&mut self, text: &str, bounds: (i32, i32, i32, i32), bold: bool) -> HWND {
        self.add(
            w!("STATIC"),
            text,
            WINDOW_STYLE(SS_NOPREFIX),
            WINDOW_EX_STYLE(0),
            0,
            bounds,
            bold,
        )
    }

    pub fn combo(&mut self, id: usize, items: &[String], bounds: (i32, i32, i32, i32)) -> HWND {
        let (x, y, w, _) = bounds;
        let hwnd = self.add(
            w!("COMBOBOX"),
            "",
            WS_TABSTOP | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            WINDOW_EX_STYLE(0),
            id,
            (x, y, w, COMBO_DROP_HEIGHT),
            false,
        );
        for item in items {
            let item_w = wide(item);
            unsafe {
                SendMessageW(
                    hwnd,
                    CB_ADDSTRING,
                    WPARAM(0),
                    LPARAM(item_w.as_ptr() as isize),
                );
            }
        }
        self.theme(hwnd, w!("DarkMode_CFD"));
        hwnd
    }

    pub fn checkbox(&mut self, id: usize, text: &str, bounds: (i32, i32, i32, i32)) -> HWND {
        let hwnd = self.add(
            w!("BUTTON"),
            text,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            id,
            bounds,
            false,
        );
        // Themed checkboxes ignore WM_CTLCOLORSTATIC text colors: use the classic
        // style in dark mode so the label stays readable.
        if self.dark {
            unsafe {
                let _ = SetWindowTheme(hwnd, w!(""), w!(""));
            }
        }
        hwnd
    }

    pub fn button(
        &mut self,
        id: usize,
        text: &str,
        default: bool,
        bounds: (i32, i32, i32, i32),
    ) -> HWND {
        let style = if default {
            BS_DEFPUSHBUTTON
        } else {
            BS_PUSHBUTTON
        };
        let hwnd = self.add(
            w!("BUTTON"),
            text,
            WS_TABSTOP | WINDOW_STYLE(style as u32),
            WINDOW_EX_STYLE(0),
            id,
            bounds,
            false,
        );
        self.theme(hwnd, w!("DarkMode_Explorer"));
        hwnd
    }

    pub fn edit(
        &mut self,
        id: usize,
        style: WINDOW_STYLE,
        ex_style: WINDOW_EX_STYLE,
        bounds: (i32, i32, i32, i32),
    ) -> HWND {
        let hwnd = self.add(
            w!("EDIT"),
            "",
            WINDOW_STYLE(ES_AUTOHSCROLL as u32) | style,
            ex_style,
            id,
            bounds,
            false,
        );
        self.theme(hwnd, w!("DarkMode_CFD"));
        hwnd
    }

    pub fn list_box(
        &mut self,
        id: usize,
        style: WINDOW_STYLE,
        ex_style: WINDOW_EX_STYLE,
        bounds: (i32, i32, i32, i32),
    ) -> HWND {
        let hwnd = self.add(w!("LISTBOX"), "", style, ex_style, id, bounds, false);
        self.theme(hwnd, w!("DarkMode_Explorer"));
        hwnd
    }

    /// Moves a child to new logical bounds (used by resizable windows).
    pub fn place(&mut self, hwnd: HWND, bounds: (i32, i32, i32, i32)) {
        let (x, y, w, h) = bounds;
        if let Some(child) = self.children.iter_mut().find(|c| c.hwnd == hwnd) {
            child.bounds = RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
        }
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                None,
                scale(x, self.dpi),
                scale(y, self.dpi),
                scale(w, self.dpi),
                scale(h, self.dpi),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    /// Re-creates the fonts and re-scales every child for a new DPI.
    pub fn apply_dpi(&mut self, dpi: u32) {
        if dpi == self.dpi {
            return;
        }
        unsafe {
            let _ = DeleteObject(self.font);
            let _ = DeleteObject(self.font_bold);
        }
        self.dpi = dpi;
        self.font = create_font(dpi, false);
        self.font_bold = create_font(dpi, true);

        for child in &self.children {
            let font = if child.bold {
                self.font_bold
            } else {
                self.font
            };
            let b = child.bounds;
            unsafe {
                SendMessageW(child.hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
                let _ = SetWindowPos(
                    child.hwnd,
                    None,
                    scale(b.left, dpi),
                    scale(b.top, dpi),
                    scale(b.right - b.left, dpi),
                    scale(b.bottom - b.top, dpi),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

pub fn combo_select(hwnd: HWND, index: usize) {
    unsafe {
        SendMessageW(hwnd, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    }
}

/// Selected combo box index (`None` when nothing is selected).
pub fn combo_selected(hwnd: HWND) -> Option<usize> {
    let index = unsafe { SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 };
    usize::try_from(index).ok()
}

pub fn set_checked(hwnd: HWND, checked: bool) {
    unsafe {
        SendMessageW(hwnd, BM_SETCHECK, WPARAM(checked as usize), LPARAM(0));
    }
}

pub fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
}
