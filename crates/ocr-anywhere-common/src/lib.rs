pub mod config;
pub mod i18n;

pub use config::{
    set_windows_startup, AppConfig, HotkeyConfig, MonitorTarget, ThemeMode, MAX_HISTORY_RANGE,
};
pub use i18n::{Language, LanguageMode};
