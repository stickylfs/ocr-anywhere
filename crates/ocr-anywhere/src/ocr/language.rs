use std::collections::HashMap;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::SoftwareBitmap;
use windows::Media::Ocr::OcrEngine;

use super::engine::{recognize_bitmap, score_recognized_text, RecognizedText};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// No Windows OCR language pack is installed.
    NoLanguages,
    /// The recognizer failed.
    Failed(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrError::NoLanguages => write!(f, "No OCR language engines are installed"),
            OcrError::Failed(e) => write!(f, "OCR recognition failed: {}", e),
        }
    }
}

fn failed(e: windows::core::Error) -> OcrError {
    OcrError::Failed(e.to_string())
}

pub struct OcrManager {
    cached_engines: HashMap<String, OcrEngine>,
    user_profile_engine: Option<OcrEngine>,
}

impl Default for OcrManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrManager {
    pub fn new() -> Self {
        Self {
            cached_engines: HashMap::new(),
            user_profile_engine: None,
        }
    }

    /// Lists all installed Windows OCR language tags (e.g. ["en-US", "es-ES"]).
    pub fn get_available_languages() -> Vec<String> {
        let mut languages = Vec::new();
        if let Ok(list) = OcrEngine::AvailableRecognizerLanguages() {
            if let Ok(size) = list.Size() {
                for i in 0..size {
                    if let Ok(lang) = list.GetAt(i) {
                        if let Ok(tag) = lang.LanguageTag() {
                            languages.push(tag.to_string());
                        }
                    }
                }
            }
        }
        languages
    }

    /// Gets or creates an OcrEngine for a specific language tag.
    pub fn get_or_create_engine(&mut self, language_tag: &str) -> Option<&OcrEngine> {
        if !self.cached_engines.contains_key(language_tag) {
            let htag = HSTRING::from(language_tag);
            if let Ok(lang) = Language::CreateLanguage(&htag) {
                if let Ok(engine) = OcrEngine::TryCreateFromLanguage(&lang) {
                    self.cached_engines.insert(language_tag.to_string(), engine);
                }
            }
        }
        self.cached_engines.get(language_tag)
    }

    /// Gets or creates the default user profile OCR engine.
    pub fn get_user_profile_engine(&mut self) -> Option<&OcrEngine> {
        if self.user_profile_engine.is_none() {
            if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
                self.user_profile_engine = Some(engine);
            }
        }
        self.user_profile_engine.as_ref()
    }

    /// Executes OCR with the requested language setting ("auto" or a language tag).
    /// An unavailable language falls back to automatic selection.
    pub fn recognize(
        &mut self,
        bitmap: &SoftwareBitmap,
        language_mode: &str,
    ) -> Result<RecognizedText, OcrError> {
        if !language_mode.eq_ignore_ascii_case("auto") && !language_mode.is_empty() {
            if let Some(engine) = self.get_or_create_engine(language_mode) {
                return recognize_bitmap(engine, bitmap).map_err(failed);
            }
        }

        // Automatic mode: Evaluate candidate recognizers
        let available = Self::get_available_languages();

        if available.is_empty() {
            // Fallback to user profile languages
            return match self.get_user_profile_engine() {
                Some(engine) => recognize_bitmap(engine, bitmap).map_err(failed),
                None => Err(OcrError::NoLanguages),
            };
        }

        if available.len() == 1 {
            let tag = &available[0];
            if let Some(engine) = self.get_or_create_engine(tag) {
                return recognize_bitmap(engine, bitmap).map_err(failed);
            }
        }

        // Multiple languages installed: run multi-engine scoring prioritizing system UI language
        let system_lang = ocr_anywhere_common::Language::detect_system_language();
        let pref_prefix = match system_lang {
            ocr_anywhere_common::Language::Es => "es",
            ocr_anywhere_common::Language::En => "en",
        };

        let mut sorted_available = available.clone();
        sorted_available.sort_by_key(|tag| {
            if tag.to_lowercase().starts_with(pref_prefix) {
                0
            } else {
                1
            }
        });

        let mut best_result: Option<RecognizedText> = None;
        let mut highest_score: f64 = -1.0;

        for tag in &sorted_available {
            if let Some(engine) = self.get_or_create_engine(tag) {
                if let Ok(result) = recognize_bitmap(engine, bitmap) {
                    let mut score = score_recognized_text(&result);
                    // Slight prior preference if engine matches system UI language
                    if tag.to_lowercase().starts_with(pref_prefix) {
                        score *= 1.10;
                    }
                    if score > highest_score {
                        highest_score = score;
                        best_result = Some(result);
                    }
                }
            }
        }

        best_result.ok_or_else(|| {
            OcrError::Failed("no installed OCR language could process the image".to_string())
        })
    }
}
