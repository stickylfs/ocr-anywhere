# Security Policy

## Reporting a vulnerability

Please report security issues privately through **GitHub Security Advisories** ("Report a vulnerability" in the *Security* tab of this repository) instead of opening a public issue. You will receive an answer as soon as possible.

## Supported versions

Only the latest release receives fixes.

## Privacy and security properties

- **No network access**: the program contains no networking code and sends no telemetry. Recognition runs locally with the Windows OCR engine.
- **No screenshots on disk**: the screen snapshot exists in memory only while you select a region and is discarded right after recognition.
- **Encrypted history**: captured text is stored in `%LOCALAPPDATA%\ocr-anywhere\history.dat`, encrypted with Windows DPAPI (`CryptProtectData`) for the current user account; there are no hard-coded keys. A history file that cannot be decrypted (for example, copied from another account) is renamed to `history.dat.bak` instead of being overwritten. History can be disabled in Settings.
- **Clipboard**: recognized text is placed on the clipboard, where other applications (including clipboard history and sync, if enabled in Windows) can read it.
- **Standard user rights**: no administrator privileges are required. The only registry entries are the *Settings → Apps* uninstall entry and, if enabled, the `HKCU\...\Run` startup value; both are removed on uninstall.
