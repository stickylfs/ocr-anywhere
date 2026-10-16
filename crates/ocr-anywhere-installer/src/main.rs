#![windows_subsystem = "windows"]

//! `ocr-anywhere-setup.exe`: self-contained per-user installer and uninstaller.
//!
//! The generic install flow lives in `setup.rs`; this file adds the OCR
//! Anywhere settings, startup entry and texts.

mod dialog;
mod registry;
mod setup;
mod shortcut;

use ocr_anywhere_common::config::RUN_VALUE_NAME;
use ocr_anywhere_common::AppConfig;
use registry::delete_reg_value;
use setup::{App, File, Language, Paths, Shortcut, Texts};
use std::fs;
use std::process::Command;
use windows::Win32::System::Registry::HKEY_CURRENT_USER;

const APP_EXE_NAME: &str = "ocr-anywhere.exe";
const RUN_REG_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// Reads an existing configuration without creating one.
fn existing_config() -> Option<AppConfig> {
    fs::read_to_string(AppConfig::get_config_path())
        .ok()
        .map(|c| AppConfig::parse_ini(&c))
}

struct OcrAnywhere;

impl App for OcrAnywhere {
    const ID: &'static str = "ocr-anywhere";
    const DISPLAY_NAME: &'static str = "OCR Anywhere";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const FILES: &'static [File] = &[
        File {
            name: APP_EXE_NAME,
            bytes: include_bytes!(concat!(env!("OUT_DIR"), "/ocr-anywhere.exe")),
        },
        File {
            name: "app.ico",
            bytes: include_bytes!("../../../assets/app.ico"),
        },
    ];
    const PROCESSES: &'static [&'static str] = &[APP_EXE_NAME];

    fn texts(lang: Language) -> Texts {
        let hotkey = existing_config()
            .unwrap_or_default()
            .hotkey
            .display_string();
        match lang {
            Language::En => Texts {
                description: format!("Captures text from anywhere on your screen with a global shortcut ({hotkey}), using the OCR engine built into Windows.\n\n100% offline: no telemetry and no network connections. The capture history is encrypted with Windows DPAPI."),
                usage: format!("OCR Anywhere runs in the notification area. Press {hotkey} and select an area of the screen to copy its text to the clipboard."),
                uninstall_question: "The program, its settings and the capture history will be deleted.".into(),
                uninstall_done: "The application, its shortcuts, settings and history have been removed.".into(),
                arch_reason: None,
            },
            Language::Es => Texts {
                description: format!("Captura texto de cualquier parte de la pantalla con un atajo global ({hotkey}), usando el motor de OCR integrado en Windows.\n\n100% sin conexión: sin telemetría ni conexiones de red. El historial de capturas se cifra con Windows DPAPI."),
                usage: format!("OCR Anywhere se ejecuta en el área de notificación. Pulsa {hotkey} y selecciona una zona de la pantalla para copiar su texto al portapapeles."),
                uninstall_question: "Se eliminarán el programa, su configuración y el historial de capturas.".into(),
                uninstall_done: "Se han eliminado la aplicación, sus accesos directos, la configuración y el historial.".into(),
                arch_reason: None,
            },
        }
    }

    fn install(_paths: &Paths, _lang: Language) -> Result<(), String> {
        // Default configuration on first install; the app itself keeps the
        // startup entry in sync with its `start_with_windows` setting.
        if !AppConfig::get_config_path().exists() {
            let _ = AppConfig::default().save();
        }
        Ok(())
    }

    fn uninstall(_paths: &Paths) {
        let _ = delete_reg_value(HKEY_CURRENT_USER, RUN_REG_KEY, RUN_VALUE_NAME);
    }

    fn shortcuts(paths: &Paths, lang: Language) -> Vec<Shortcut> {
        vec![
            Shortcut {
                name: Self::DISPLAY_NAME.into(),
                target: paths.install_dir.join(APP_EXE_NAME),
                description: lang
                    .pick(
                        "Capture text from anywhere on your screen",
                        "Captura texto de cualquier parte de la pantalla",
                    )
                    .into(),
            },
            Shortcut {
                name: lang
                    .pick("Settings (config.ini)", "Configuración (config.ini)")
                    .into(),
                target: AppConfig::get_config_path(),
                description: lang
                    .pick(
                        "Edit the OCR Anywhere settings",
                        "Editar la configuración de OCR Anywhere",
                    )
                    .into(),
            },
        ]
    }

    fn launch(paths: &Paths) {
        let _ = Command::new(paths.install_dir.join(APP_EXE_NAME)).spawn();
    }
}

fn main() {
    setup::run::<OcrAnywhere>();
}
