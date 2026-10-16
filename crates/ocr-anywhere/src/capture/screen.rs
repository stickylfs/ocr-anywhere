use windows::core::Result as WinResult;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};

use super::monitor::MonitorInfo;

const CAPTUREBLT: u32 = 0x40000000;

/// Freeze-frame snapshot of one monitor. Owns the GDI bitmap used to paint the
/// selection overlay, so it is intentionally not `Clone` (a copy would delete
/// the same bitmap twice).
pub struct CapturedFrame {
    pub monitor: MonitorInfo,
    pub width: i32,
    pub height: i32,
    /// Top-down 32-bit BGRA pixels.
    pub bgra_pixels: Vec<u8>,
    pub hbitmap_handle: isize,
}

impl Drop for CapturedFrame {
    fn drop(&mut self) {
        if self.hbitmap_handle != 0 {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.hbitmap_handle as _));
            }
            self.hbitmap_handle = 0;
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapturedRegion {
    pub width: u32,
    pub height: u32,
    pub bgra_data: Vec<u8>,
}

/// Takes an instantaneous freeze-frame snapshot of the specified monitor using GDI BitBlt.
pub fn capture_monitor_snapshot(monitor: &MonitorInfo) -> Result<CapturedFrame, String> {
    let width = monitor.width();
    let height = monitor.height();

    if width <= 0 || height <= 0 {
        return Err("Invalid monitor dimensions".to_string());
    }

    unsafe {
        let hdc_screen = GetDC(HWND::default());
        if hdc_screen.is_invalid() {
            return Err("Failed to get screen DC".to_string());
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_invalid() {
            let _ = ReleaseDC(HWND::default(), hdc_screen);
            return Err("Failed to create memory DC".to_string());
        }

        let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);
        if hbitmap.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(HWND::default(), hdc_screen);
            return Err("Failed to create compatible bitmap".to_string());
        }

        let old_obj = SelectObject(hdc_mem, hbitmap);

        // Blit the monitor region into memory bitmap
        let blt_res = BitBlt(
            hdc_mem,
            0,
            0,
            width,
            height,
            hdc_screen,
            monitor.rect.left,
            monitor.rect.top,
            SRCCOPY | windows::Win32::Graphics::Gdi::ROP_CODE(CAPTUREBLT),
        );

        if let Err(e) = blt_res {
            let _ = SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(hbitmap);
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(HWND::default(), hdc_screen);
            return Err(format!("BitBlt screen capture failed: {}", e));
        }

        // Prepare BITMAPINFO for 32-bit BGRA top-down retrieval
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // Negative indicates top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: (width as u32) * (height as u32) * 4,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            ..Default::default()
        };

        let mut bgra_pixels = vec![0u8; width as usize * height as usize * 4];

        let scan_lines = GetDIBits(
            hdc_mem,
            hbitmap,
            0,
            height as u32,
            Some(bgra_pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Clean up GDI objects (keep hbitmap inside CapturedFrame for fast rendering)
        let _ = SelectObject(hdc_mem, old_obj);
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(HWND::default(), hdc_screen);

        if scan_lines == 0 {
            let _ = DeleteObject(hbitmap);
            return Err("GetDIBits failed to extract pixel buffer".to_string());
        }

        Ok(CapturedFrame {
            monitor: monitor.clone(),
            width,
            height,
            bgra_pixels,
            hbitmap_handle: hbitmap.0 as isize,
        })
    }
}

/// Crops a sub-region (in monitor-relative coordinates) from a CapturedFrame.
pub fn crop_region(
    frame: &CapturedFrame,
    rel_rect: windows::Win32::Foundation::RECT,
) -> Result<CapturedRegion, String> {
    let mut x1 = rel_rect.left.max(0).min(frame.width);
    let mut y1 = rel_rect.top.max(0).min(frame.height);
    let mut x2 = rel_rect.right.max(0).min(frame.width);
    let mut y2 = rel_rect.bottom.max(0).min(frame.height);

    if x1 > x2 {
        std::mem::swap(&mut x1, &mut x2);
    }
    if y1 > y2 {
        std::mem::swap(&mut y1, &mut y2);
    }

    let crop_w = (x2 - x1) as usize;
    let crop_h = (y2 - y1) as usize;

    if crop_w == 0 || crop_h == 0 {
        return Err("Selected crop region is empty".to_string());
    }

    let mut cropped_data = Vec::with_capacity(crop_w * crop_h * 4);
    let stride = (frame.width * 4) as usize;

    for row in y1..y2 {
        let start = (row as usize * stride) + (x1 as usize * 4);
        let end = start + (crop_w * 4);
        if end <= frame.bgra_pixels.len() {
            cropped_data.extend_from_slice(&frame.bgra_pixels[start..end]);
        }
    }

    Ok(CapturedRegion {
        width: crop_w as u32,
        height: crop_h as u32,
        bgra_data: cropped_data,
    })
}

/// Converts a CapturedRegion into a WinRT SoftwareBitmap.
pub fn create_software_bitmap(region: &CapturedRegion) -> WinResult<SoftwareBitmap> {
    let ibuffer = CryptographicBuffer::CreateFromByteArray(&region.bgra_data)?;
    SoftwareBitmap::CreateCopyFromBuffer(
        &ibuffer,
        BitmapPixelFormat::Bgra8,
        region.width as i32,
        region.height as i32,
    )
}
