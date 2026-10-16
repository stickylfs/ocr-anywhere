use ocr_anywhere::settings::storage::{AppSettings, HotkeyConfig, MonitorTarget, ThemeMode};

#[test]
fn test_default_settings() {
    let settings = AppSettings::default();
    assert_eq!(settings.theme, ThemeMode::System);
    assert_eq!(settings.monitor_target, MonitorTarget::Cursor);
    assert_eq!(settings.ocr_language, "auto");
    assert!(settings.start_with_windows);
    assert!(settings.history_enabled);
    assert_eq!(settings.max_history_entries, 100);
}

#[test]
fn test_hotkey_display_string() {
    // Default is MOD_CONTROL (0x2) | MOD_ALT (0x1) with VK 'O' (0x4F)
    assert_eq!(HotkeyConfig::default().display_string(), "Ctrl+Alt+O");

    let ctrl_shift_f1 = HotkeyConfig {
        modifiers: 0x0002 | 0x0004,
        vk_code: 0x70,
    };
    assert_eq!(ctrl_shift_f1.display_string(), "Ctrl+Shift+F1");
}

#[test]
fn test_settings_ini_roundtrip() {
    let settings = AppSettings {
        theme: ThemeMode::Dark,
        ocr_language: "es-ES".to_string(),
        max_history_entries: 250,
        hotkey: HotkeyConfig::from_str_parts("ctrl+shift", "T"),
        ..Default::default()
    };

    let restored = AppSettings::parse_ini(&settings.to_ini_string());
    assert_eq!(restored, settings);
    assert_eq!(restored.hotkey.display_string(), "Ctrl+Shift+T");
}
