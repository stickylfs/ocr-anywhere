use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::crypto::{decrypt_dpapi, encrypt_dpapi};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: String,
    /// Capture time in seconds since the Unix epoch (UTC).
    pub timestamp: u64,
    pub text: String,
}

/// Capture history, persisted as DPAPI-encrypted JSON (newest entry first).
#[derive(Debug, Clone)]
pub struct HistoryStore {
    pub entries: Vec<HistoryEntry>,
    pub max_entries: usize,
    file_path: PathBuf,
    /// Where an unreadable history file was moved when it was opened.
    pub recovered_backup: Option<PathBuf>,
}

pub fn default_history_path() -> PathBuf {
    ocr_anywhere_common::AppConfig::get_history_path()
}

impl HistoryStore {
    /// Opens the history stored at `file_path`.
    ///
    /// A file that cannot be decrypted or parsed (e.g. copied from another
    /// Windows account) is renamed to `*.bak` instead of being overwritten.
    pub fn new(file_path: PathBuf, max_entries: usize) -> Self {
        let mut store = Self {
            entries: Vec::new(),
            max_entries,
            file_path,
            recovered_backup: None,
        };
        if store.load().is_err() {
            let backup = store.file_path.with_extension("dat.bak");
            if fs::rename(&store.file_path, &backup).is_ok() {
                store.recovered_backup = Some(backup);
            }
            store.entries.clear();
        }
        store
    }

    pub fn load_default() -> Self {
        Self::new(default_history_path(), 100)
    }

    pub fn load(&mut self) -> Result<(), String> {
        let encrypted_bytes = match fs::read(&self.file_path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.entries.clear();
                return Ok(());
            }
            Err(e) => return Err(format!("Failed to read history file: {}", e)),
        };

        if encrypted_bytes.is_empty() {
            self.entries.clear();
            return Ok(());
        }

        let decrypted_bytes = decrypt_dpapi(&encrypted_bytes)
            .map_err(|e| format!("Failed to decrypt history: {}", e))?;

        self.entries = serde_json::from_slice(&decrypted_bytes)
            .map_err(|e| format!("Failed to parse history entries: {}", e))?;
        self.prune();
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create history directory: {}", e))?;
        }

        let json_bytes = serde_json::to_vec(&self.entries)
            .map_err(|e| format!("Failed to serialize history: {}", e))?;
        let encrypted =
            encrypt_dpapi(&json_bytes).map_err(|e| format!("Failed to encrypt history: {}", e))?;

        // Write to a temporary file first so a crash never leaves a truncated history.
        let temp_path = self.file_path.with_extension("dat.tmp");
        fs::write(&temp_path, encrypted)
            .and_then(|_| fs::rename(&temp_path, &self.file_path))
            .map_err(|e| format!("Failed to write history file: {}", e))
    }

    /// Generates an ID that is unique even for captures within the same second
    /// or after deletions.
    fn next_id(&self, now_ms: u128) -> String {
        let mut candidate = now_ms;
        loop {
            let id = candidate.to_string();
            if !self.entries.iter().any(|e| e.id == id) {
                return id;
            }
            candidate += 1;
        }
    }

    pub fn add_entry(&mut self, text: String) -> Result<HistoryEntry, String> {
        if text.trim().is_empty() {
            return Err("Cannot add empty entry".to_string());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let entry = HistoryEntry {
            id: self.next_id(now.as_millis()),
            timestamp: now.as_secs(),
            text,
        };

        self.entries.insert(0, entry.clone());
        self.prune();
        self.save()?;
        Ok(entry)
    }

    pub fn delete_entry(&mut self, id: &str) -> bool {
        let initial_len = self.entries.len();
        self.entries.retain(|e| e.id != id);
        if self.entries.len() != initial_len {
            let _ = self.save();
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.entries.clear();
        self.save()
    }

    pub fn set_max_entries(&mut self, max: usize) {
        self.max_entries = max;
        let before = self.entries.len();
        self.prune();
        if self.entries.len() != before {
            let _ = self.save();
        }
    }

    fn prune(&mut self) {
        if self.max_entries > 0 && self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }
    }
}
