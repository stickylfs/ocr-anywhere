# Contributing to OCR Anywhere

Thank you for your interest in contributing! Bug reports, recognition improvements, translations, fixes and documentation improvements are all welcome.

## Development setup

### Prerequisites

- Windows 10 (1809+) or 11 with at least one OCR-capable language installed
- [Rust](https://rustup.rs/) 1.82 or newer
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the *Desktop development with C++* workload (MSVC linker and Windows SDK)

### Build and test

```bash
git clone https://github.com/<your-username>/ocr-anywhere.git
cd ocr-anywhere

cargo test --workspace            # run all test suites
cargo run -p ocr-anywhere         # run the tray app (debug build)

# Release installer: it embeds the application, so build it first.
cargo build --release -p ocr-anywhere
cargo build --release -p ocr-anywhere-installer
```

The installer is written to `target/release/ocr-anywhere-setup.exe`. Building the installer before the application prints a warning and produces an installer that refuses to run.

The OCR integration test renders text and recognizes it with `Windows.Media.Ocr`; it is skipped when no OCR language is installed.

## Project layout

- `crates/ocr-anywhere-common`: `config.ini` model (`AppConfig`), hotkey parsing and the application strings (`i18n.rs`).
- `crates/ocr-anywhere/src`
  - `main.rs`: hidden host window, tray icon, hotkey and message loop.
  - `app/`: capture workflow (snapshot → overlay → crop → OCR → clipboard → history).
  - `capture/`, `overlay/`: GDI screen snapshot and the selection overlay.
  - `ocr/`: preprocessing, recognition and multi-language scoring, text clean-up.
  - `history/`: DPAPI-encrypted history store.
  - `ui/`: settings and history windows (DPI-aware Win32 controls), and `alert.rs` for non-blocking error dialogs.
- `crates/ocr-anywhere-installer`: per-user installer and uninstaller. `setup.rs` holds the app-independent install flow; `main.rs` describes the app (files, registrations and texts).

## Pull request checklist

1. Format the code: `cargo fmt --all`
2. No lint warnings: `cargo clippy --workspace --all-targets -- -D warnings`
3. Tests pass: `cargo test --workspace`
4. Update `CHANGELOG.md` for user-visible changes.
5. Describe what the change does and why in the pull request.

## Code style

- Window procedures must not touch the shared state while a capture is running (see `capture_in_progress`): the selection overlay runs a nested message loop.
- Problems the user has to fix are reported with `ui::alert::show_alert` (a dialog on its own thread, never from inside a window procedure's borrow of the app state); minor ones with a tray notification.
- Text clean-up heuristics in `ocr/engine.rs` must stay generic; add a test for every new rule.
- User-facing text goes in `ocr-anywhere-common/src/i18n.rs` (installer texts in `ocr-anywhere-installer/src/main.rs`), in English and Spanish.

## Releasing

1. Update the version in the workspace `Cargo.toml` and add a section to `CHANGELOG.md`.
2. Tag the commit (`git tag v1.2.3 && git push --tags`).
3. The **Release** workflow builds the x64, x86 and ARM64 installers, generates `SHA256SUMS.txt` and publishes a GitHub release.

## License

By contributing, you agree that your contributions are licensed under the MIT License, like the rest of the project.
