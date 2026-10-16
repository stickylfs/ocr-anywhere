use ocr_anywhere::history::crypto::{decrypt_dpapi, encrypt_dpapi};

#[test]
fn test_dpapi_encryption_decryption_roundtrip() {
    let original_text = "This is confidential extracted OCR text from screen: 1234-5678-ABCD";
    let encrypted = encrypt_dpapi(original_text.as_bytes()).expect("Encryption must succeed");

    assert!(!encrypted.is_empty(), "Ciphertext should not be empty");
    assert_ne!(
        encrypted,
        original_text.as_bytes(),
        "Ciphertext must not be plaintext"
    );

    let decrypted = decrypt_dpapi(&encrypted).expect("Decryption must succeed");
    let decrypted_text = String::from_utf8(decrypted).expect("Must be valid UTF-8");

    assert_eq!(decrypted_text, original_text);
}

#[test]
fn test_dpapi_empty_input() {
    let encrypted = encrypt_dpapi(&[]).expect("Empty input encryption should succeed");
    assert!(encrypted.is_empty());

    let decrypted = decrypt_dpapi(&[]).expect("Empty input decryption should succeed");
    assert!(decrypted.is_empty());
}

#[test]
fn test_dpapi_unicode_support() {
    let unicode_sample = "Texto en español con tildes: ñ, á, é, í, ó, ú, ü, ¿?, ¡!";
    let encrypted = encrypt_dpapi(unicode_sample.as_bytes()).expect("Encryption must succeed");
    let decrypted = decrypt_dpapi(&encrypted).expect("Decryption must succeed");
    let text = String::from_utf8(decrypted).unwrap();
    assert_eq!(text, unicode_sample);
}
