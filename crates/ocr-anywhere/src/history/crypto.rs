use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// Encrypts raw bytes using Windows DPAPI (tied to current Windows user account).
pub fn encrypt_dpapi(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    if plaintext.is_empty() {
        return Ok(Vec::new());
    }

    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len() as u32,
            pbData: plaintext.as_ptr() as *mut u8,
        };

        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let res = CryptProtectData(
            &in_blob,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );

        if let Err(e) = res {
            return Err(format!("CryptProtectData failed to encrypt data: {}", e));
        }

        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let result = slice.to_vec();

        // Free the buffer allocated by DPAPI
        let _ = LocalFree(HLOCAL(out_blob.pbData as _));

        Ok(result)
    }
}

/// Decrypts raw DPAPI ciphertext into plaintext bytes.
pub fn decrypt_dpapi(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }

    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: ciphertext.len() as u32,
            pbData: ciphertext.as_ptr() as *mut u8,
        };

        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let mut descr = PWSTR::null();

        let res = CryptUnprotectData(
            &in_blob,
            Some(&mut descr),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        );

        if let Err(e) = res {
            return Err(format!("CryptUnprotectData failed to decrypt data: {}", e));
        }

        if !descr.is_null() {
            let _ = LocalFree(HLOCAL(descr.0 as _));
        }

        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let result = slice.to_vec();

        // Free the buffer allocated by DPAPI
        let _ = LocalFree(HLOCAL(out_blob.pbData as _));

        Ok(result)
    }
}
