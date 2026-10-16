# Changelog

All notable changes to this project are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [1.0.0] - 2026-10-14

First public release.

### Added
- Global hotkey capture with a freeze-frame selection overlay and recognition with `Windows.Media.Ocr`.
- Automatic multi-language recognition with result scoring.
- Tiny or low-contrast text, such as desktop icon labels, is retried with a larger, contrast-enhanced image when the first recognition finds nothing.
- DPAPI-encrypted, searchable capture history.
- Settings window: interface language, theme, capture monitor (including specific monitors), capture shortcut, start with Windows (on by default), OCR language and history size. Changes apply immediately.
- Alerts with a direct fix when the shortcut is taken by another app (opens Settings) or no OCR language is installed (opens the Windows language settings), and when the clipboard is busy, a capture fails or the settings cannot be saved. Minor issues use tray notifications.
- English and Spanish user interface.
- Per-user installer (`ocr-anywhere-setup.exe`) for x64, x86 and ARM64 that launches the app when it finishes, with update/repair, unattended mode and clean uninstall.
