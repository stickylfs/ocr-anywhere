//! End-to-end recognition test: renders text with GDI, then runs it through the
//! same preprocessing and `Windows.Media.Ocr` pipeline as a real capture.
//! Skipped when the machine has no OCR language installed (e.g. Server Core CI).

use ocr_anywhere::capture::screen::{create_software_bitmap, CapturedRegion};
use ocr_anywhere::ocr::{max_image_dimension, preprocess_region_for_ocr, OcrError, OcrManager};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC,
    DeleteObject, FillRect, GetDC, GetDIBits, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    TextOutW, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, FW_NORMAL, TRANSPARENT,
};

/// Renders `text` in black Segoe UI on a white background.
fn render_text(text: &str, width: i32, height: i32, font_px: i32) -> CapturedRegion {
    render_styled(text, width, height, font_px, 0x00FF_FFFF, 0, None)
}

/// Renders `text` with the given colors (`0x00BBGGRR`) and an optional 1 px
/// drop shadow, like desktop icon labels.
fn render_styled(
    text: &str,
    width: i32,
    height: i32,
    font_px: i32,
    background: u32,
    foreground: u32,
    shadow: Option<u32>,
) -> CapturedRegion {
    unsafe {
        let screen = GetDC(HWND::default());
        let dc = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old_bitmap = SelectObject(dc, bitmap);

        let brush = CreateSolidBrush(COLORREF(background));
        let rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush);

        let font = CreateFontW(
            -font_px,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            0,
            0,
            0,
            5,
            0,
            w!("Segoe UI"),
        );
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT);
        let text_w: Vec<u16> = text.encode_utf16().collect();
        let y = (height - font_px) / 2;
        if let Some(shadow) = shadow {
            SetTextColor(dc, COLORREF(shadow));
            let _ = TextOutW(dc, 11, y + 1, &text_w);
        }
        SetTextColor(dc, COLORREF(foreground));
        let _ = TextOutW(dc, 10, y, &text_w);

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(dc, old_font);
        let _ = DeleteObject(font);
        SelectObject(dc, old_bitmap);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(dc);
        ReleaseDC(HWND::default(), screen);

        CapturedRegion {
            width: width as u32,
            height: height as u32,
            bgra_data: pixels,
        }
    }
}

#[test]
fn test_recognizes_rendered_text() {
    if OcrManager::get_available_languages().is_empty() {
        println!("No Windows OCR language installed; skipping.");
        return;
    }

    // Small UI-sized text exercises the upscaling path.
    let region = render_text("Hello World 2026", 220, 30, 13);
    let prepared = preprocess_region_for_ocr(&region, max_image_dimension());
    let bitmap = create_software_bitmap(&prepared).expect("software bitmap");

    let mut manager = OcrManager::new();
    let result = match manager.recognize(&bitmap, "auto") {
        Ok(result) => result,
        Err(OcrError::NoLanguages) => return,
        Err(e) => panic!("recognition failed: {}", e),
    };

    let text = result.text.to_lowercase();
    assert!(text.contains("hello"), "recognized: {:?}", result.text);
    assert!(text.contains("2026"), "recognized: {:?}", result.text);
}

#[test]
fn test_recognizes_desktop_icon_label() {
    if OcrManager::get_available_languages().is_empty() {
        println!("No Windows OCR language installed; skipping.");
        return;
    }

    // White 9 pt label with a dark shadow on a light wallpaper, as under a
    // desktop icon: tiny, low-contrast text.
    let region = render_styled(
        "CJ 3D Model",
        90,
        22,
        12,
        0x00D8_B070,
        0x00FF_FFFF,
        Some(0x0040_4040),
    );
    let mut manager = OcrManager::new();
    let text = ocr_anywhere::ocr::recognize_region(&mut manager, &region, "auto")
        .expect("recognition")
        .text;
    // "3D" is often read as "30" at this size; the words must come through.
    let lower = text.to_lowercase();
    assert!(
        lower.contains("cj") && lower.contains("model"),
        "recognized: {:?}",
        text
    );
}
