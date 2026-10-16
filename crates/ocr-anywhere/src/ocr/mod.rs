pub mod engine;
pub mod language;
pub mod preprocess;

pub use engine::{format_ocr_lines, recognize_bitmap, score_recognized_text, RecognizedText};
pub use language::{OcrError, OcrManager};
pub use preprocess::{enhance_region_for_ocr, preprocess_region_for_ocr};

use crate::capture::screen::{create_software_bitmap, CapturedRegion};

/// Largest image side accepted by `Windows.Media.Ocr` (10000 px on current
/// Windows versions, queried at runtime).
pub fn max_image_dimension() -> u32 {
    windows::Media::Ocr::OcrEngine::MaxImageDimension().unwrap_or(2600)
}

/// Recognizes the text in a captured region.
///
/// When the standard pass finds nothing, the region is retried enhanced
/// (larger, grayscale, stretched contrast): tiny or low-contrast text, such as
/// desktop icon labels, is often only read that way.
pub fn recognize_region(
    manager: &mut OcrManager,
    region: &CapturedRegion,
    language_mode: &str,
) -> Result<RecognizedText, OcrError> {
    let max_dimension = max_image_dimension();
    let passes: [&dyn Fn() -> CapturedRegion; 2] = [
        &|| preprocess_region_for_ocr(region, max_dimension),
        &|| enhance_region_for_ocr(region, max_dimension),
    ];

    let mut last = None;
    for prepare in passes {
        let bitmap = create_software_bitmap(&prepare())
            .map_err(|e| OcrError::Failed(format!("Bitmap error: {e}")))?;
        let recognized = manager.recognize(&bitmap, language_mode)?;
        if !recognized.text.trim().is_empty() {
            return Ok(recognized);
        }
        last = Some(recognized);
    }
    Ok(last.expect("at least one pass"))
}
