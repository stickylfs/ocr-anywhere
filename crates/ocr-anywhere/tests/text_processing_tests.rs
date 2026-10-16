use ocr_anywhere::ocr::engine::{
    format_ocr_lines, normalize_ocr_timestamp, score_recognized_text, RecognizedText,
};

fn recognized(text: &str, language_tag: &str) -> RecognizedText {
    RecognizedText {
        text: text.to_string(),
        language_tag: language_tag.to_string(),
        line_count: text.lines().count(),
        word_count: text.split_whitespace().count(),
    }
}

#[test]
fn test_format_ocr_lines_trims_and_joins_with_crlf() {
    let raw_lines = vec![
        "  First paragraph line 1.   ".to_string(),
        "First paragraph line 2.".to_string(),
        "".to_string(),
        "   Second paragraph begins here.   ".to_string(),
    ];
    assert_eq!(
        format_ocr_lines(&raw_lines),
        "First paragraph line 1.\r\nFirst paragraph line 2.\r\nSecond paragraph begins here."
    );
}

#[test]
fn test_unicode_text_is_preserved() {
    let lines = vec![
        "Configuración del sistema".to_string(),
        "Reconocimiento óptico de caracteres: ñ, ü, ¿?, ¡!".to_string(),
        "TIPO".to_string(),
    ];
    assert_eq!(
        format_ocr_lines(&lines),
        "Configuración del sistema\r\nReconocimiento óptico de caracteres: ñ, ü, ¿?, ¡!\r\nTIPO"
    );
}

#[test]
fn test_clean_line_artifacts() {
    let raw = vec![
        "G] ocr-anywhere-setup.exe".to_string(),
        "@ocr-anywhere-setup-x86.exe".to_string(),
        "[1] Normal footnote with brackets".to_string(),
        "@user_profile_handle".to_string(),
    ];
    let formatted = format_ocr_lines(&raw);
    let lines: Vec<&str> = formatted.lines().collect();

    // Icon glyphs misread as characters are stripped from file names...
    assert_eq!(lines[0], "ocr-anywhere-setup.exe");
    assert_eq!(lines[1], "ocr-anywhere-setup-x86.exe");
    // ...but real brackets and handles are kept.
    assert_eq!(lines[2], "[1] Normal footnote with brackets");
    assert_eq!(lines[3], "@user_profile_handle");
}

#[test]
fn test_time_after_date_is_repaired() {
    assert_eq!(
        normalize_ocr_timestamp("09/09/2026 16'04"),
        "09/09/2026 16:04"
    );
    assert_eq!(
        normalize_ocr_timestamp("09/09/2026 16•04"),
        "09/09/2026 16:04"
    );
    assert_eq!(
        normalize_ocr_timestamp("09/09/2026 1604"),
        "09/09/2026 16:04"
    );
    assert_eq!(
        normalize_ocr_timestamp("09/09/20261603"),
        "09/09/2026 16:03"
    );
    assert_eq!(
        normalize_ocr_timestamp("09/09/2026 1 5:49"),
        "09/09/2026 15:49"
    );
    assert_eq!(
        normalize_ocr_timestamp("09/09/2026 15:48"),
        "09/09/2026 15:48"
    );
}

#[test]
fn test_numbers_without_a_date_are_untouched() {
    assert_eq!(
        normalize_ocr_timestamp("Total 1604 units"),
        "Total 1604 units"
    );
    assert_eq!(normalize_ocr_timestamp("Price: 12.50"), "Price: 12.50");
    // Decimals and ranges after a date are not mistaken for times.
    assert_eq!(
        normalize_ocr_timestamp("2026-01-05 12.50"),
        "2026-01-05 12.50"
    );
    assert_eq!(
        normalize_ocr_timestamp("2026-01-05 10-20"),
        "2026-01-05 10-20"
    );
}

#[test]
fn test_score_prefers_valid_text() {
    let valid = recognized(
        "The quick brown fox jumps over the lazy dog. https://example.com/test",
        "en-US",
    );
    let gibberish = recognized("^%$ #@ *& ~`", "es-ES");

    assert_eq!(score_recognized_text(&recognized("   ", "en-US")), 0.0);
    assert!(score_recognized_text(&valid) > score_recognized_text(&gibberish));
    assert!(score_recognized_text(&valid) > 50.0);
}

#[test]
fn test_score_penalizes_digit_intrusion_and_unnatural_accents() {
    let corrupted = recognized("F«ha de modificaci6n\r\nAplicaciön\r\nTamaöo", "es-ES");
    let clean = recognized("Fecha de modificación\r\nAplicación\r\nTamaño", "es-ES");

    let (score_corrupted, score_clean) = (
        score_recognized_text(&corrupted),
        score_recognized_text(&clean),
    );
    assert!(
        score_clean > score_corrupted * 2.0,
        "clean={}, corrupted={}",
        score_clean,
        score_corrupted
    );
}
