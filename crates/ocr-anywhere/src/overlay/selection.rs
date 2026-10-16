use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen,
    CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, FillRect, GetDC, GetStockObject,
    InvalidateRect, Rectangle, ReleaseDC, SelectObject, AC_SRC_OVER, BLENDFUNCTION, HBITMAP,
    HBRUSH, HDC, HGDIOBJ, NULL_BRUSH, PAINTSTRUCT, PS_SOLID, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW,
    PostQuitMessage, RegisterClassExW, SetCursor, SetForegroundWindow, SetWindowPos, ShowWindow,
    TranslateMessage, CREATESTRUCTW, HICON, HWND_TOPMOST, IDC_CROSS, MSG, SWP_SHOWWINDOW, SW_SHOW,
    WA_INACTIVE, WM_ACTIVATE, WM_CAPTURECHANGED, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN, WM_SETCURSOR,
    WNDCLASSEXW, WNDCLASS_STYLES, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::capture::screen::CapturedFrame;

const OVERLAY_CLASS_NAME: PCWSTR = w!("OCRAnywhere_OverlayClass");

/// Selections smaller than this (in pixels) are treated as accidental clicks.
const MIN_SELECTION: i32 = 4;

/// Darkening applied outside the selection (0-255).
const DIM_ALPHA: u8 = 110;

/// Selection border color (BGR).
const BORDER_COLOR: COLORREF = COLORREF(0x00EB_8000);

struct OverlayState {
    /// Borrowed from `run_selection_overlay`, which outlives the window.
    frame: *const CapturedFrame,
    start_pt: Option<POINT>,
    current_pt: Option<POINT>,
    is_dragging: bool,
    selection_result: Option<RECT>,
    /// Set once the window is being destroyed (destruction itself sends
    /// `WM_ACTIVATE`, which must not trigger a second `DestroyWindow`).
    closing: bool,
    /// 1x1 black bitmap stretched with `AlphaBlend` to dim the screen.
    dark_bitmap: HBITMAP,
}

impl OverlayState {
    /// Closes the overlay exactly once.
    unsafe fn close(&mut self, hwnd: HWND) {
        if !self.closing {
            self.closing = true;
            let _ = DestroyWindow(hwnd);
        }
    }

    fn frame(&self) -> &CapturedFrame {
        unsafe { &*self.frame }
    }

    /// Current selection in window coordinates, clamped to the frame.
    fn selection(&self) -> Option<RECT> {
        let (s, c) = (self.start_pt?, self.current_pt?);
        let (w, h) = (self.frame().width, self.frame().height);
        Some(RECT {
            left: s.x.min(c.x).clamp(0, w),
            top: s.y.min(c.y).clamp(0, h),
            right: s.x.max(c.x).clamp(0, w),
            bottom: s.y.max(c.y).clamp(0, h),
        })
    }
}

impl Drop for OverlayState {
    fn drop(&mut self) {
        if !self.dark_bitmap.is_invalid() {
            unsafe {
                let _ = DeleteObject(self.dark_bitmap);
            }
        }
    }
}

fn point_from_lparam(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam.0 & 0xFFFF) as i16 as i32,
        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
    }
}

unsafe fn create_dark_bitmap() -> HBITMAP {
    let hdc_screen = GetDC(HWND::default());
    let bitmap = CreateCompatibleBitmap(hdc_screen, 1, 1);
    let hdc = CreateCompatibleDC(hdc_screen);
    let old = SelectObject(hdc, bitmap);
    let brush = CreateSolidBrush(COLORREF(0));
    FillRect(
        hdc,
        &RECT {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        },
        brush,
    );
    let _ = DeleteObject(brush);
    SelectObject(hdc, old);
    let _ = DeleteDC(hdc);
    ReleaseDC(HWND::default(), hdc_screen);
    bitmap
}

/// Shows a full-screen selection overlay over the frozen `frame` and runs a
/// modal message loop until the user drags a rectangle or cancels (Esc,
/// right-click, switching windows).
///
/// Returns the selected region in monitor-relative coordinates.
pub fn run_selection_overlay(frame: &CapturedFrame) -> Option<RECT> {
    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(overlay_wnd_proc),
            hInstance: hinstance.into(),
            hIcon: HICON::default(),
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszClassName: OVERLAY_CLASS_NAME,
            ..Default::default()
        };
        // Fails harmlessly with ERROR_CLASS_ALREADY_EXISTS after the first capture.
        let _ = RegisterClassExW(&wnd_class);

        let state_ptr = Box::into_raw(Box::new(OverlayState {
            frame,
            start_pt: None,
            current_pt: None,
            is_dragging: false,
            selection_result: None,
            closing: false,
            dark_bitmap: create_dark_bitmap(),
        }));

        let rect = frame.monitor.rect;
        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            OVERLAY_CLASS_NAME,
            w!("OCR Anywhere Selection"),
            WS_POPUP,
            rect.left,
            rect.top,
            frame.width,
            frame.height,
            HWND::default(),
            None,
            hinstance,
            Some(state_ptr as *const _),
        ) {
            Ok(h) => h,
            Err(_) => {
                drop(Box::from_raw(state_ptr));
                return None;
            }
        };

        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            rect.left,
            rect.top,
            frame.width,
            frame.height,
            SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        // Take keyboard focus so Esc reaches the overlay instead of the app below.
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let state = Box::from_raw(state_ptr);
        state.selection_result
    }
}

unsafe fn paint(hwnd: HWND, state: &OverlayState) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let (w, h) = (state.frame().width, state.frame().height);

    // Compose into a back buffer to avoid flicker.
    let hdc_mem = CreateCompatibleDC(hdc);
    let back_buffer = CreateCompatibleBitmap(hdc, w, h);
    let old_buf = SelectObject(hdc_mem, back_buffer);

    let hdc_src = CreateCompatibleDC(hdc);
    let old_src = SelectObject(hdc_src, HGDIOBJ(state.frame().hbitmap_handle as _));
    let _ = BitBlt(hdc_mem, 0, 0, w, h, hdc_src, 0, 0, SRCCOPY);
    SelectObject(hdc_src, old_src);
    let _ = DeleteDC(hdc_src);

    let hdc_dark = CreateCompatibleDC(hdc);
    let old_dark = SelectObject(hdc_dark, state.dark_bitmap);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: DIM_ALPHA,
        AlphaFormat: 0,
    };
    let dim = |dc: HDC, x: i32, y: i32, cx: i32, cy: i32| {
        if cx > 0 && cy > 0 {
            let _ = AlphaBlend(dc, x, y, cx, cy, hdc_dark, 0, 0, 1, 1, blend);
        }
    };

    match state.selection() {
        Some(r) => {
            // Dim everything around the (clear) selection.
            dim(hdc_mem, 0, 0, w, r.top);
            dim(hdc_mem, 0, r.bottom, w, h - r.bottom);
            dim(hdc_mem, 0, r.top, r.left, r.bottom - r.top);
            dim(hdc_mem, r.right, r.top, w - r.right, r.bottom - r.top);

            let pen = CreatePen(PS_SOLID, 2, BORDER_COLOR);
            let old_pen = SelectObject(hdc_mem, pen);
            let old_brush = SelectObject(hdc_mem, GetStockObject(NULL_BRUSH));
            let _ = Rectangle(hdc_mem, r.left, r.top, r.right, r.bottom);
            SelectObject(hdc_mem, old_brush);
            SelectObject(hdc_mem, old_pen);
            let _ = DeleteObject(pen);
        }
        None => dim(hdc_mem, 0, 0, w, h),
    }

    SelectObject(hdc_dark, old_dark);
    let _ = DeleteDC(hdc_dark);

    let _ = BitBlt(hdc, 0, 0, w, h, hdc_mem, 0, 0, SRCCOPY);
    SelectObject(hdc_mem, old_buf);
    let _ = DeleteObject(back_buffer);
    let _ = DeleteDC(hdc_mem);
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr: *mut OverlayState = crate::get_window_userdata(hwnd);

    match msg {
        WM_CREATE => {
            let create_struct = lparam.0 as *const CREATESTRUCTW;
            if let Some(cs) = create_struct.as_ref() {
                crate::set_window_userdata(hwnd, cs.lpCreateParams as *mut OverlayState);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let _ = SetCursor(LoadCursorW(None, IDC_CROSS).unwrap_or_default());
            LRESULT(1)
        }
        // The whole client area is painted in WM_PAINT.
        WM_ERASEBKGND => LRESULT(1),
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                if let Some(state) = state_ptr.as_mut() {
                    state.close(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            if let Some(state) = state_ptr.as_mut() {
                state.close(hwnd);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // Cancel when the user switches to another window (Alt+Tab, Win key...).
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                if let Some(state) = state_ptr.as_mut() {
                    state.close(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(state) = state_ptr.as_mut() {
                let pt = point_from_lparam(lparam);
                state.start_pt = Some(pt);
                state.current_pt = Some(pt);
                state.is_dragging = true;
                SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(state) = state_ptr.as_mut() {
                if state.is_dragging {
                    state.current_pt = Some(point_from_lparam(lparam));
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(state) = state_ptr.as_mut() {
                if state.is_dragging {
                    state.is_dragging = false;
                    state.current_pt = Some(point_from_lparam(lparam));
                    state.selection_result = state.selection().filter(|r| {
                        r.right - r.left >= MIN_SELECTION && r.bottom - r.top >= MIN_SELECTION
                    });
                    if GetCapture() == hwnd {
                        let _ = ReleaseCapture();
                    }
                    state.close(hwnd);
                }
                // A button release without a press on the overlay (mouse already
                // down when it appeared) is ignored.
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // Mouse capture stolen mid-drag (e.g. by a system dialog): cancel.
            if let Some(state) = state_ptr.as_mut() {
                if state.is_dragging && HWND(lparam.0 as *mut _) != hwnd {
                    state.is_dragging = false;
                    state.close(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            if let Some(state) = state_ptr.as_ref() {
                paint(hwnd, state);
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_DESTROY => {
            crate::clear_window_userdata(hwnd);
            // Ends the modal loop in `run_selection_overlay`.
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
