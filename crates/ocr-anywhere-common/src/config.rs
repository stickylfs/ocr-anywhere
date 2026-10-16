use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};

use crate::i18n::LanguageMode;

/// Registry value name used under `HKCU\...\Run` for "start with Windows".
pub const RUN_VALUE_NAME: &str = "ocr-anywhere";

/// Smallest and largest accepted `[history] max_entries` values.
pub const MAX_HISTORY_RANGE: std::ops::RangeInclusive<usize> = 1..=10_000;

const VK_F1: u32 = 0x70;
const VK_F24: u32 = 0x87;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl std::str::FromStr for ThemeMode {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "light" | "claro" => Ok(ThemeMode::Light),
            "dark" | "oscuro" => Ok(ThemeMode::Dark),
            _ => Ok(ThemeMode::System),
        }
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThemeMode::System => write!(f, "system"),
            ThemeMode::Light => write!(f, "light"),
            ThemeMode::Dark => write!(f, "dark"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MonitorTarget {
    #[default]
    Cursor,
    Primary,
    /// Zero-based monitor index in enumeration order.
    Index(usize),
}

impl std::str::FromStr for MonitorTarget {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "primary" | "principal" => Ok(MonitorTarget::Primary),
            other => Ok(other
                .parse::<usize>()
                .map(MonitorTarget::Index)
                .unwrap_or(MonitorTarget::Cursor)),
        }
    }
}

impl std::fmt::Display for MonitorTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonitorTarget::Cursor => write!(f, "cursor"),
            MonitorTarget::Primary => write!(f, "primary"),
            MonitorTarget::Index(i) => write!(f, "{}", i),
        }
    }
}

/// Global hotkey: `RegisterHotKey` modifier flags plus a virtual-key code
/// (A-Z, 0-9 or F1-F24).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub modifiers: u32,
    pub vk_code: u32,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        HotkeyConfig {
            modifiers: MOD_CONTROL.0 | MOD_ALT.0,
            vk_code: 0x4F, // 'O'
        }
    }
}

impl HotkeyConfig {
    const MODIFIER_NAMES: [(u32, &'static str, &'static str); 4] = [
        (MOD_CONTROL.0, "Ctrl", "ctrl"),
        (MOD_ALT.0, "Alt", "alt"),
        (MOD_SHIFT.0, "Shift", "shift"),
        (MOD_WIN.0, "Win", "win"),
    ];

    /// Human-readable form, e.g. `Ctrl+Alt+O`.
    pub fn display_string(&self) -> String {
        let mut parts: Vec<String> = Self::MODIFIER_NAMES
            .iter()
            .filter(|(flag, _, _)| self.modifiers & flag != 0)
            .map(|(_, name, _)| name.to_string())
            .collect();
        parts.push(self.key_to_string());
        parts.join("+")
    }

    /// INI form of the modifiers, e.g. `ctrl+alt`.
    pub fn modifiers_to_string(&self) -> String {
        Self::MODIFIER_NAMES
            .iter()
            .filter(|(flag, _, _)| self.modifiers & flag != 0)
            .map(|(_, _, ini)| *ini)
            .collect::<Vec<_>>()
            .join("+")
    }

    /// INI / display form of the key, e.g. `O`, `5` or `F9`.
    pub fn key_to_string(&self) -> String {
        match self.vk_code {
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(self.vk_code).unwrap_or('O').to_string(),
            VK_F1..=VK_F24 => format!("F{}", self.vk_code - VK_F1 + 1),
            _ => "O".to_string(),
        }
    }

    /// Parses a key name (`A`-`Z`, `0`-`9`, `F1`-`F24`).
    pub fn parse_key(key_str: &str) -> Option<u32> {
        let key = key_str.trim().to_uppercase();
        if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
            return (1..=24).contains(&n).then_some(VK_F1 + n - 1);
        }
        let mut chars = key.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_alphanumeric() => Some(c as u32),
            _ => None,
        }
    }

    /// Builds a hotkey from INI strings, falling back to the defaults for invalid
    /// values. Letter and digit keys always require Ctrl, Alt or Win so the
    /// hotkey cannot swallow normal typing.
    pub fn from_str_parts(modifiers_str: &str, key_str: &str) -> Self {
        let mut modifiers = 0u32;
        for part in modifiers_str.split('+') {
            match part.trim().to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
                "alt" => modifiers |= MOD_ALT.0,
                "shift" => modifiers |= MOD_SHIFT.0,
                "win" | "windows" => modifiers |= MOD_WIN.0,
                _ => {}
            }
        }

        let default = HotkeyConfig::default();
        let vk_code = Self::parse_key(key_str).unwrap_or(default.vk_code);
        let is_function_key = (VK_F1..=VK_F24).contains(&vk_code);
        if !is_function_key && modifiers & (MOD_CONTROL.0 | MOD_ALT.0 | MOD_WIN.0) == 0 {
            modifiers = default.modifiers;
        }

        HotkeyConfig { modifiers, vk_code }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub language: LanguageMode,
    pub theme: ThemeMode,
    pub start_with_windows: bool,
    pub monitor_target: MonitorTarget,
    pub hotkey: HotkeyConfig,
    /// `auto` or a Windows OCR language tag such as `en-US`.
    pub ocr_language: String,
    pub history_enabled: bool,
    pub max_history_entries: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            language: LanguageMode::Auto,
            theme: ThemeMode::System,
            start_with_windows: true,
            monitor_target: MonitorTarget::Cursor,
            hotkey: HotkeyConfig::default(),
            ocr_language: "auto".to_string(),
            history_enabled: true,
            max_history_entries: 100,
        }
    }
}

/// Parses an INI boolean (`true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`).
fn parse_bool(val: &str) -> Option<bool> {
    match val.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Extracts the value part of an INI line: strips surrounding quotes and any
/// trailing `#` / `;` comment outside of quotes.
fn clean_value(raw: &str) -> &str {
    let raw = raw.trim();
    for quote in ['"', '\''] {
        if let Some(rest) = raw.strip_prefix(quote) {
            return match rest.find(quote) {
                Some(end) => rest[..end].trim(),
                None => rest.trim(),
            };
        }
    }
    match raw.find(['#', ';']) {
        Some(idx) => raw[..idx].trim(),
        None => raw,
    }
}

impl AppConfig {
    pub fn get_app_dir() -> PathBuf {
        let base = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("ocr-anywhere")
    }

    pub fn get_config_path() -> PathBuf {
        Self::get_app_dir().join("config.ini")
    }

    pub fn get_history_path() -> PathBuf {
        Self::get_app_dir().join("history.dat")
    }

    /// Loads `%LOCALAPPDATA%\ocr-anywhere\config.ini`, creating it with the
    /// defaults when missing.
    pub fn load() -> Self {
        let config_path = Self::get_config_path();
        match fs::read_to_string(&config_path) {
            Ok(content) => Self::parse_ini(&content),
            Err(_) => {
                let default_cfg = AppConfig::default();
                if !config_path.exists() {
                    let _ = default_cfg.save();
                }
                default_cfg
            }
        }
    }

    /// Parses INI content. Unknown keys and invalid values keep their defaults.
    pub fn parse_ini(content: &str) -> Self {
        let mut config = AppConfig::default();
        let mut current_section = String::new();
        let mut hotkey_modifiers = config.hotkey.modifiers_to_string();
        let mut hotkey_key = config.hotkey.key_to_string();

        for line in content.lines() {
            let line = line.trim().trim_start_matches('\u{FEFF}');
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].trim().to_lowercase();
                continue;
            }

            let Some((key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            let val = clean_value(raw_val);

            match (current_section.as_str(), key.as_str()) {
                ("general", "language") => config.language = val.parse().unwrap_or_default(),
                ("general", "theme") => config.theme = val.parse().unwrap_or_default(),
                ("general", "start_with_windows") => {
                    if let Some(b) = parse_bool(val) {
                        config.start_with_windows = b;
                    }
                }
                ("general", "monitor_target") => {
                    config.monitor_target = val.parse().unwrap_or_default();
                }
                ("hotkey", "modifiers") => hotkey_modifiers = val.to_string(),
                ("hotkey", "key") => hotkey_key = val.to_string(),
                ("ocr", "language") => {
                    if !val.is_empty() {
                        config.ocr_language = val.to_string();
                    }
                }
                ("history", "enabled") => {
                    if let Some(b) = parse_bool(val) {
                        config.history_enabled = b;
                    }
                }
                ("history", "max_entries") => {
                    if let Ok(n) = val.parse::<usize>() {
                        config.max_history_entries =
                            n.clamp(*MAX_HISTORY_RANGE.start(), *MAX_HISTORY_RANGE.end());
                    }
                }
                _ => {}
            }
        }

        config.hotkey = HotkeyConfig::from_str_parts(&hotkey_modifiers, &hotkey_key);
        config
    }

    /// Serializes the configuration as a commented INI file.
    pub fn to_ini_string(&self) -> String {
        format!(
            r#"# ==============================================================================
# OCR Anywhere configuration
# Location: %LOCALAPPDATA%\ocr-anywhere\config.ini
# Restart OCR Anywhere after editing this file by hand (changes made in the
# Settings window apply immediately).
# ==============================================================================

[general]
# Interface language: "auto" (Windows language), "es" or "en"
language = {}

# Visual theme: "system" (follows Windows), "light" or "dark"
theme = {}

# Start automatically when you sign in to Windows: true | false
start_with_windows = {}

# Monitor to capture: "cursor" (monitor under the mouse), "primary" or an index (0, 1, ...)
monitor_target = {}

[hotkey]
# Modifier keys: any combination of ctrl, alt, shift and win (e.g. "ctrl+shift")
modifiers = {}

# Trigger key: A-Z, 0-9 or F1-F24
key = {}

[ocr]
# OCR language: "auto" (tries every installed Windows OCR language) or a tag such as "en-US"
language = {}

[history]
# Keep a local history encrypted with Windows DPAPI: true | false
enabled = {}

# Number of captures kept ({} to {})
max_entries = {}
"#,
            self.language,
            self.theme,
            self.start_with_windows,
            self.monitor_target,
            self.hotkey.modifiers_to_string(),
            self.hotkey.key_to_string(),
            self.ocr_language,
            self.history_enabled,
            MAX_HISTORY_RANGE.start(),
            MAX_HISTORY_RANGE.end(),
            self.max_history_entries
        )
    }

    /// Writes the configuration to `%LOCALAPPDATA%\ocr-anywhere\config.ini`.
    pub fn save(&self) -> std::io::Result<()> {
        fs::create_dir_all(Self::get_app_dir())?;
        fs::write(Self::get_config_path(), self.to_ini_string())
    }
}

/// Adds or removes the `HKCU\...\Run` entry that starts `exe_path` at sign-in.
pub fn set_windows_startup(enable: bool, exe_path: &std::path::Path) -> Result<(), u32> {
    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let app_name: Vec<u16> = RUN_VALUE_NAME
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        if enable {
            let path_str = format!("\"{}\"", exe_path.display());
            let wide_path: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
            let mut hkey = HKEY::default();

            let res = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                0,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut hkey,
                None,
            );
            if res != ERROR_SUCCESS {
                return Err(res.0);
            }

            let set_res = RegSetValueExW(
                hkey,
                PCWSTR(app_name.as_ptr()),
                0,
                REG_SZ,
                Some(std::slice::from_raw_parts(
                    wide_path.as_ptr() as *const u8,
                    wide_path.len() * std::mem::size_of::<u16>(),
                )),
            );
            let _ = RegCloseKey(hkey);
            if set_res != ERROR_SUCCESS {
                return Err(set_res.0);
            }
        } else {
            let mut hkey = HKEY::default();
            let open_res = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            );
            if open_res == ERROR_SUCCESS {
                let _ = RegDeleteValueW(hkey, PCWSTR(app_name.as_ptr()));
                let _ = RegCloseKey(hkey);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ini_parse() {
        let sample = r#"
[general]
language = es
theme = dark
start_with_windows = yes
monitor_target = primary

[hotkey]
modifiers = ctrl+alt+shift
key = X

[ocr]
language = "es-ES" # Spanish

[history]
enabled = false
max_entries = 50
"#;
        let cfg = AppConfig::parse_ini(sample);
        assert_eq!(cfg.language, LanguageMode::Es);
        assert_eq!(cfg.theme, ThemeMode::Dark);
        assert!(cfg.start_with_windows);
        assert_eq!(cfg.monitor_target, MonitorTarget::Primary);
        assert_eq!(cfg.hotkey.display_string(), "Ctrl+Alt+Shift+X");
        assert_eq!(cfg.ocr_language, "es-ES");
        assert!(!cfg.history_enabled);
        assert_eq!(cfg.max_history_entries, 50);
    }

    #[test]
    fn test_ini_roundtrip() {
        let cfg = AppConfig {
            language: LanguageMode::En,
            theme: ThemeMode::Light,
            start_with_windows: true,
            monitor_target: MonitorTarget::Index(2),
            hotkey: HotkeyConfig::from_str_parts("win+shift", "F9"),
            ocr_language: "en-US".to_string(),
            history_enabled: false,
            max_history_entries: 250,
        };
        assert_eq!(AppConfig::parse_ini(&cfg.to_ini_string()), cfg);
        assert_eq!(
            AppConfig::parse_ini(&AppConfig::default().to_ini_string()),
            AppConfig::default()
        );
    }

    #[test]
    fn test_invalid_values_fall_back() {
        let cfg = AppConfig::parse_ini(
            "[general]\nstart_with_windows = maybe\n[hotkey]\nmodifiers = shift\nkey = ??\n[history]\nmax_entries = 0\n",
        );
        assert!(cfg.start_with_windows);
        // A bare Shift+letter hotkey would block typing: defaults are used instead.
        assert_eq!(cfg.hotkey, HotkeyConfig::default());
        assert_eq!(cfg.max_history_entries, 1);
    }

    #[test]
    fn test_hotkey_keys() {
        assert_eq!(HotkeyConfig::parse_key("f12"), Some(0x7B));
        assert_eq!(HotkeyConfig::parse_key("F25"), None);
        assert_eq!(HotkeyConfig::parse_key("7"), Some(0x37));
        assert_eq!(HotkeyConfig::parse_key("ab"), None);
        assert_eq!(
            HotkeyConfig::from_str_parts("", "F8").display_string(),
            "F8"
        );
    }
}
