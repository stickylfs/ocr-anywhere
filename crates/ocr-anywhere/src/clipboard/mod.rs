use std::ptr::copy_nonoverlapping;
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;

/// Other applications (clipboard managers, RDP) may hold the clipboard briefly.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(25);

/// Copies `text` to the Windows clipboard as `CF_UNICODETEXT`.
pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes_len = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        // The clipboard needs an owner window: with a NULL owner, EmptyClipboard
        // leaves no owner and SetClipboardData may fail.
        let owner = crate::host_window();
        let mut opened = false;
        for _ in 0..OPEN_ATTEMPTS {
            if OpenClipboard(owner).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(OPEN_RETRY_DELAY);
        }
        if !opened {
            return Err("The clipboard is in use by another application".to_string());
        }

        let result = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|e| format!("Failed to empty the clipboard: {}", e))?;

            let hglobal = GlobalAlloc(GMEM_MOVEABLE, bytes_len)
                .map_err(|e| format!("Failed to allocate clipboard memory: {}", e))?;
            let free = |h: HGLOBAL| {
                let _ = GlobalFree(h);
            };

            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                free(hglobal);
                return Err("Failed to lock clipboard memory".to_string());
            }
            copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes_len);
            let _ = GlobalUnlock(hglobal);

            // On success the system owns the memory; on failure we must free it.
            if SetClipboardData(CF_UNICODETEXT, HANDLE(hglobal.0)).is_err() {
                free(hglobal);
                return Err("Failed to set clipboard data".to_string());
            }
            Ok(())
        })();

        let _ = CloseClipboard();
        result
    }
}
