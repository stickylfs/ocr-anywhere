use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LanguageMode {
    #[default]
    Auto,
    En,
    Es,
}

impl std::str::FromStr for LanguageMode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "es" | "spanish" | "español" | "castellano" => Ok(LanguageMode::Es),
            "en" | "english" | "ingles" | "inglés" => Ok(LanguageMode::En),
            _ => Ok(LanguageMode::Auto),
        }
    }
}

impl std::fmt::Display for LanguageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LanguageMode::Auto => write!(f, "auto"),
            LanguageMode::Es => write!(f, "es"),
            LanguageMode::En => write!(f, "en"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    En,
    Es,
}

/// Defines `Language` methods returning a static string per language.
macro_rules! strings {
    ($($(#[$doc:meta])* $name:ident => $en:expr, $es:expr;)*) => {
        impl Language {
            $(
                $(#[$doc])*
                pub fn $name(&self) -> &'static str {
                    match self {
                        Language::En => $en,
                        Language::Es => $es,
                    }
                }
            )*
        }
    };
}

impl Language {
    /// Determines the active language based on a `LanguageMode` setting.
    pub fn resolve(mode: LanguageMode) -> Self {
        match mode {
            LanguageMode::En => Language::En,
            LanguageMode::Es => Language::Es,
            LanguageMode::Auto => Self::detect_system_language(),
        }
    }

    /// Detects the Windows UI language.
    pub fn detect_system_language() -> Self {
        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }

            let lang_id = unsafe { GetUserDefaultUILanguage() };
            Self::from_lang_id(lang_id)
        }
        #[cfg(not(windows))]
        {
            Language::En
        }
    }

    /// Maps a Windows LANGID to a `Language`. The primary language ID is
    /// `lang_id & 0x3FF`; 0x0A is LANG_SPANISH (Spain, Mexico, Argentina, ...).
    pub fn from_lang_id(lang_id: u16) -> Self {
        if lang_id & 0x3FF == 0x0A {
            Language::Es
        } else {
            Language::En
        }
    }

    pub fn tray_extract_text(&self, hotkey: &str) -> String {
        match self {
            Language::En => format!("Capture text ({})", hotkey),
            Language::Es => format!("Capturar texto ({})", hotkey),
        }
    }

    pub fn tray_tooltip(&self, hotkey: &str) -> String {
        match self {
            Language::En => format!("OCR Anywhere: press {} to capture text", hotkey),
            Language::Es => format!("OCR Anywhere: pulsa {} para capturar texto", hotkey),
        }
    }

    pub fn hotkey_in_use(&self, hotkey: &str) -> String {
        match self {
            Language::En => format!(
                "The shortcut {} is already used by another application. Choose a different one in Settings.",
                hotkey
            ),
            Language::Es => format!(
                "El atajo {} ya lo usa otra aplicación. Elige otro en Ajustes.",
                hotkey
            ),
        }
    }

    pub fn hotkey_conflict_title(&self, hotkey: &str) -> String {
        match self {
            Language::En => format!("The shortcut {} is already in use", hotkey),
            Language::Es => format!("El atajo {} ya está en uso", hotkey),
        }
    }

    pub fn already_running_content(&self, hotkey: &str) -> String {
        match self {
            Language::En => format!(
                "Press {} to capture text, or use its icon in the notification area.",
                hotkey
            ),
            Language::Es => format!(
                "Pulsa {} para capturar texto o usa su icono del área de notificación.",
                hotkey
            ),
        }
    }

    pub fn settings_save_failed_content(&self, path: &str) -> String {
        match self {
            Language::En => format!(
                "The changes apply until OCR Anywhere is closed. Check that this file can be written:\n{}",
                path
            ),
            Language::Es => format!(
                "Los cambios se aplican hasta que se cierre OCR Anywhere. Comprueba que se puede escribir en este archivo:\n{}",
                path
            ),
        }
    }

    pub fn settings_monitor_index(&self, index: usize) -> String {
        match self {
            Language::En => format!("Monitor {}", index + 1),
            Language::Es => format!("Monitor {}", index + 1),
        }
    }
}

strings! {
    // --- Tray ---
    tray_history => "OCR History", "Historial OCR";
    tray_settings => "Settings...", "Ajustes...";
    tray_exit => "Exit", "Salir";

    // --- Notifications (tray balloons) ---
    no_text_found => "No text was recognized in the selected area.",
        "No se ha reconocido texto en la zona seleccionada.";
    no_ocr_languages => "No OCR language is installed. Add a language in Windows Settings > Time & language > Language & region.",
        "No hay ningún idioma de OCR instalado. Añade un idioma en Configuración de Windows > Hora e idioma > Idioma y región.";
    history_save_failed => "The text was copied, but it could not be saved in the history.",
        "El texto se ha copiado, pero no se ha podido guardar en el historial.";
    history_recovered => "The capture history could not be read (for example, it was copied from another Windows account). It was kept as history.dat.bak and a new history was started.",
        "No se ha podido leer el historial de capturas (por ejemplo, porque se copió de otra cuenta de Windows). Se ha conservado como history.dat.bak y se ha empezado uno nuevo.";
    startup_entry_failed => "Start with Windows could not be enabled.",
        "No se ha podido activar el inicio con Windows.";

    // --- Alerts (dialogs) ---
    alert_close => "Close", "Cerrar";
    already_running_title => "OCR Anywhere is already running", "OCR Anywhere ya se está ejecutando";
    startup_failed => "OCR Anywhere could not start", "OCR Anywhere no se ha podido iniciar";
    hotkey_conflict_content => "Another application registered it first, so OCR Anywhere cannot capture text with it. Choose a different shortcut in Settings.",
        "Otra aplicación lo ha registrado antes, así que OCR Anywhere no puede capturar texto con él. Elige otro atajo en Ajustes.";
    change_shortcut => "Change shortcut...", "Cambiar atajo...";
    no_ocr_languages_title => "No OCR language is installed", "No hay ningún idioma de OCR instalado";
    no_ocr_languages_content => "Windows needs at least one language with its OCR feature to recognize text. Add one (for example English or Spanish) in Settings > Time & language > Language & region.",
        "Windows necesita al menos un idioma con su función de OCR para reconocer texto. Añade uno (por ejemplo, español o inglés) en Configuración > Hora e idioma > Idioma y región.";
    open_language_settings => "Open language settings", "Abrir configuración de idioma";
    capture_failed => "The text could not be captured", "No se ha podido capturar el texto";
    capture_failed_content => "Try again. If the problem continues, the technical details below can help to report it.",
        "Vuelve a intentarlo. Si el problema continúa, los detalles técnicos ayudan a informar de él.";
    clipboard_failed => "The text could not be copied", "No se ha podido copiar el texto";
    clipboard_failed_content => "Another application is using the clipboard. Try again in a moment.",
        "Otra aplicación está usando el portapapeles. Vuelve a intentarlo en un momento.";
    clipboard_saved_in_history => "The recognized text has been saved in the history.",
        "El texto reconocido se ha guardado en el historial.";
    settings_save_failed => "The settings could not be saved", "No se han podido guardar los ajustes";

    // --- History window ---
    history_title => "OCR History", "Historial OCR";
    history_search_placeholder => "Search history...", "Buscar en el historial...";
    history_copy => "Copy", "Copiar";
    history_delete => "Delete", "Eliminar";
    history_clear_all => "Clear all", "Borrar todo";
    history_no_items => "No captures in the history yet.", "Todavía no hay capturas en el historial.";
    history_clear_confirm => "Do you want to delete all history entries? This cannot be undone.",
        "¿Quieres borrar todas las entradas del historial? No se puede deshacer.";
    history_disabled => "History is disabled. Enable it in Settings to keep your captures.",
        "El historial está desactivado. Actívalo en Ajustes para guardar tus capturas.";

    // --- Settings window ---
    settings_title => "OCR Anywhere Settings", "Ajustes de OCR Anywhere";
    settings_group_general => "General", "General";
    settings_group_ocr => "Text recognition", "Reconocimiento de texto";
    settings_group_history => "History", "Historial";
    settings_hotkey_label => "Capture shortcut:", "Atajo de captura:";
    settings_monitor_label => "Capture monitor:", "Monitor de captura:";
    settings_monitor_cursor => "Monitor under the mouse (recommended)", "Monitor bajo el ratón (recomendado)";
    settings_monitor_primary => "Primary monitor", "Monitor principal";
    settings_ocr_lang_label => "OCR language:", "Idioma de OCR:";
    settings_ocr_lang_auto => "Automatic (all languages)", "Automático (todos los idiomas)";
    settings_ui_lang_label => "Interface language:", "Idioma de la interfaz:";
    settings_ui_lang_auto => "Automatic (Windows language)", "Automático (idioma de Windows)";
    settings_theme_label => "Theme:", "Tema:";
    settings_theme_system => "Follow Windows", "Igual que Windows";
    settings_theme_light => "Light", "Claro";
    settings_theme_dark => "Dark", "Oscuro";
    settings_startup_label => "Start automatically with Windows", "Iniciar automáticamente con Windows";
    settings_history_label => "Keep an encrypted history of captures (DPAPI)", "Guardar un historial cifrado de capturas (DPAPI)";
    settings_max_history_label => "Entries to keep:", "Entradas a conservar:";
    settings_default_suffix => " (default)", " (predeterminado)";
    settings_save => "Save", "Guardar";
    settings_cancel => "Cancel", "Cancelar";
    settings_privacy_badge => "100% offline · Zero telemetry · Zero network connections",
        "100% sin conexión · Cero telemetría · Cero conexiones de red";

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_id_detection() {
        // Spanish: Spain (0x0C0A), Mexico (0x080A), Argentina (0x2C0A), traditional sort (0x040A)
        for id in [0x0C0A, 0x080A, 0x2C0A, 0x040A] {
            assert_eq!(Language::from_lang_id(id), Language::Es);
        }
        // English and every other language default to English
        for id in [0x0409, 0x0809, 0x0C09, 0x0407, 0x040C] {
            assert_eq!(Language::from_lang_id(id), Language::En);
        }
    }

    #[test]
    fn test_language_mode_parse() {
        assert_eq!("es".parse::<LanguageMode>().unwrap(), LanguageMode::Es);
        assert_eq!("Español".parse::<LanguageMode>().unwrap(), LanguageMode::Es);
        assert_eq!("english".parse::<LanguageMode>().unwrap(), LanguageMode::En);
        assert_eq!(
            "unknown".parse::<LanguageMode>().unwrap(),
            LanguageMode::Auto
        );
    }

    #[test]
    fn test_hotkey_aware_strings() {
        assert_eq!(
            Language::En.tray_extract_text("Ctrl+Shift+T"),
            "Capture text (Ctrl+Shift+T)"
        );
    }
}
