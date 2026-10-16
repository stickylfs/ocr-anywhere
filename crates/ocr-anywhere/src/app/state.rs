use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture::monitor::get_target_monitor;
use crate::capture::screen::{capture_monitor_snapshot, crop_region};
use crate::clipboard::set_clipboard_text;
use crate::history::storage::HistoryStore;
use crate::ocr::language::{OcrError, OcrManager};
use crate::ocr::recognize_region;
use crate::overlay::selection::run_selection_overlay;
use crate::settings::storage::AppSettings;

/// Result of one capture attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// Text was recognized and copied to the clipboard.
    Copied(String),
    /// The user cancelled the selection.
    Cancelled,
    /// The selection contained no recognizable text.
    NoText,
    /// A capture is already in progress (e.g. hotkey pressed twice).
    Busy,
    /// No Windows OCR language is installed.
    NoOcrLanguages,
    /// Text was copied, but saving it in the history failed.
    CopiedHistoryFailed(String),
    /// The clipboard could not be opened (usually another app holds it).
    ClipboardFailed {
        error: String,
        /// The text was still saved in the history.
        saved_in_history: bool,
    },
    /// Screen capture or recognizer failure.
    Failed(String),
}

/// Set while a capture is running. The selection overlay runs a nested message
/// loop, so window procedures must check this before touching the coordinator
/// (a hotkey press during a capture would otherwise re-enter it).
static CAPTURE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

pub fn capture_in_progress() -> bool {
    CAPTURE_IN_PROGRESS.load(Ordering::Acquire)
}

pub struct AppCoordinator {
    pub settings: Arc<Mutex<AppSettings>>,
    pub history: Arc<Mutex<HistoryStore>>,
    ocr_manager: OcrManager,
}

impl AppCoordinator {
    pub fn new(settings: Arc<Mutex<AppSettings>>, history: Arc<Mutex<HistoryStore>>) -> Self {
        let max_entries = settings.lock().unwrap().max_history_entries;
        history.lock().unwrap().set_max_entries(max_entries);

        Self {
            settings,
            history,
            ocr_manager: OcrManager::new(),
        }
    }

    /// Applies settings that affect running state (history size).
    pub fn apply_settings(&self) {
        let max_entries = self.settings.lock().unwrap().max_history_entries;
        self.history.lock().unwrap().set_max_entries(max_entries);
    }

    /// Runs the full capture workflow:
    /// 1. Freeze-frame snapshot of the target monitor
    /// 2. Selection overlay
    /// 3. Crop, preprocess and recognize with `Windows.Media.Ocr`
    /// 4. Copy to the clipboard and store in the encrypted history
    pub fn trigger_ocr_pipeline(&mut self) -> CaptureOutcome {
        if CAPTURE_IN_PROGRESS.swap(true, Ordering::AcqRel) {
            return CaptureOutcome::Busy;
        }
        let outcome = self.run_pipeline();
        CAPTURE_IN_PROGRESS.store(false, Ordering::Release);
        outcome
    }

    fn run_pipeline(&mut self) -> CaptureOutcome {
        let (monitor_target, ocr_language, history_enabled) = {
            let s = self.settings.lock().unwrap();
            (
                s.monitor_target.clone(),
                s.ocr_language.clone(),
                s.history_enabled,
            )
        };

        let monitor = get_target_monitor(&monitor_target);
        let frame = match capture_monitor_snapshot(&monitor) {
            Ok(frame) => frame,
            Err(e) => return CaptureOutcome::Failed(format!("Screen capture error: {}", e)),
        };

        let Some(selection) = run_selection_overlay(&frame) else {
            return CaptureOutcome::Cancelled;
        };

        let cropped = match crop_region(&frame, selection) {
            Ok(region) => region,
            Err(_) => return CaptureOutcome::Cancelled,
        };
        // Release the full-screen snapshot before recognition.
        drop(frame);

        let recognized = match recognize_region(&mut self.ocr_manager, &cropped, &ocr_language) {
            Ok(r) => r,
            Err(OcrError::NoLanguages) => return CaptureOutcome::NoOcrLanguages,
            Err(e) => return CaptureOutcome::Failed(e.to_string()),
        };

        let text = recognized.text.trim();
        if text.is_empty() {
            return CaptureOutcome::NoText;
        }

        // Saved first, so the text is not lost when the clipboard is busy.
        let history_saved = history_enabled.then(|| {
            self.history
                .lock()
                .unwrap()
                .add_entry(text.to_string())
                .is_ok()
        });

        if let Err(error) = set_clipboard_text(text) {
            return CaptureOutcome::ClipboardFailed {
                error,
                saved_in_history: history_saved == Some(true),
            };
        }
        if history_saved == Some(false) {
            return CaptureOutcome::CopiedHistoryFailed(text.to_string());
        }
        CaptureOutcome::Copied(text.to_string())
    }
}
