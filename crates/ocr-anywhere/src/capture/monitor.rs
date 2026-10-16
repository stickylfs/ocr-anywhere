use windows::Win32::Foundation::{BOOL, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::settings::storage::MonitorTarget;

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub handle: isize,
    pub rect: RECT,
    pub work_rect: RECT,
    pub is_primary: bool,
    pub dpi: u32,
    pub scale_factor: f64,
}

impl MonitorInfo {
    pub fn width(&self) -> i32 {
        self.rect.right - self.rect.left
    }

    pub fn height(&self) -> i32 {
        self.rect.bottom - self.rect.top
    }
}

pub fn get_cursor_position() -> POINT {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    pt
}

pub fn get_monitor_dpi(hmonitor: HMONITOR) -> u32 {
    let mut dpi_x: u32 = 96;
    let mut dpi_y: u32 = 96;
    unsafe {
        if GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok() {
            dpi_x
        } else {
            96
        }
    }
}

pub fn get_all_monitors() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();

    unsafe extern "system" fn enum_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let monitors = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        if GetMonitorInfoW(hmonitor, &mut mi).as_bool() {
            let dpi = get_monitor_dpi(hmonitor);
            let is_primary = (mi.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1
            monitors.push(MonitorInfo {
                handle: hmonitor.0 as isize,
                rect: mi.rcMonitor,
                work_rect: mi.rcWork,
                is_primary,
                dpi,
                scale_factor: dpi as f64 / 96.0,
            });
        }
        BOOL(1)
    }

    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(enum_proc),
            LPARAM(&mut monitors as *mut Vec<MonitorInfo> as isize),
        );
    }

    monitors
}

pub fn get_target_monitor(target: &MonitorTarget) -> MonitorInfo {
    let all = get_all_monitors();
    if all.is_empty() {
        // Fallback default
        return MonitorInfo {
            handle: 0,
            rect: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_rect: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            is_primary: true,
            dpi: 96,
            scale_factor: 1.0,
        };
    }

    match target {
        MonitorTarget::Cursor => {
            let cursor = get_cursor_position();
            unsafe {
                let hmonitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
                for m in &all {
                    if m.handle == (hmonitor.0 as isize) {
                        return m.clone();
                    }
                }
            }
            all[0].clone()
        }
        MonitorTarget::Primary => {
            for m in &all {
                if m.is_primary {
                    return m.clone();
                }
            }
            all[0].clone()
        }
        MonitorTarget::Index(idx) => {
            if *idx < all.len() {
                all[*idx].clone()
            } else {
                all[0].clone()
            }
        }
    }
}
