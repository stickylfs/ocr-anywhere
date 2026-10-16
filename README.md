<p align="center">
  <a href="#readme"><img src="assets/logo.png" width="128" height="128" alt="OCR Anywhere logo" /></a>
</p>

<h1 align="center">OCR Anywhere</h1>

<p align="center">
  <strong>Copy text from anywhere on your screen with one shortcut. Offline, private, written in Rust.</strong>
</p>

<p align="center">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="#requirements"><img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6.svg?logo=windows" alt="Platform" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.82%2B-DEA584.svg?logo=rust" alt="Rust 1.82+" /></a>
  <a href="SECURITY.md"><img src="https://img.shields.io/badge/privacy-offline%20%C2%B7%20no%20telemetry-success.svg" alt="Privacy" /></a>
  <a href="#languages"><img src="https://img.shields.io/badge/languages-English%20%7C%20Espa%C3%B1ol-orange.svg" alt="Languages" /></a>
</p>

---

## Overview

**OCR Anywhere** lives in the notification area. Press **`Ctrl` + `Alt` + `O`**, drag a rectangle over any text on screen (images, videos, error dialogs, PDFs, remote desktops...) and the recognized text is copied to the clipboard.

```text
Ctrl + Alt + O  →  drag over the text  →  release  →  text is in the clipboard
```

<p align="center">
  <a href="#readme"><img src="docs/tray-icon.png" width="230" alt="OCR Anywhere icon (the square with a plus sign) in the Windows notification area" /></a>
</p>

Recognition uses the OCR engine built into Windows (`Windows.Media.Ocr`): nothing is uploaded and no extra language data has to be downloaded.

## Features

- **One shortcut**: configurable global hotkey (default `Ctrl+Alt+O`); the screen is frozen while you select, so moving content (videos, tooltips) can be captured.
- **Accurate on small text**: small selections are upscaled and sharpened before recognition; large ones are scaled to the engine limit instead of failing.
- **Automatic language selection**: tries every installed Windows OCR language and keeps the best-scoring result, so mixed Spanish/English content works without switching languages. A fixed language can be chosen instead.
- **Encrypted history**: recent captures are stored locally, encrypted with Windows DPAPI for your user account, and can be searched, copied and deleted. History can be disabled.
- **Multi-monitor and high-DPI**: captures the monitor under the mouse (or a fixed one), with per-monitor DPI awareness.
- **Light and dark themes**, following Windows or forced in Settings.
- **Feedback when something goes wrong**: problems you have to fix open a dialog with the fix one click away (shortcut taken by another app → *Change shortcut...*, no OCR language → *Open language settings*); minor ones, such as no text found, use a tray notification. If the clipboard is busy, the text is kept in the history.
- **Per-user install**: no administrator rights, optional start with Windows, clean uninstall from *Settings → Apps*.
- **Languages**: English and Spanish, following the Windows display language (configurable).

## Requirements

- Windows 10 (version 1809 or later) or Windows 11 (x64, x86 or ARM64).
- At least one OCR-capable Windows language (any display language installed with its basic typing/OCR features, e.g. English or Spanish). Add languages in *Settings → Time & language → Language & region*.

## Installation

Download the installer from the [Releases](https://github.com/stickylfs/ocr-anywhere/releases) page and run it:

| File | System |
| :--- | :--- |
| `ocr-anywhere-setup.exe` | Windows 10/11 64-bit (recommended) |
| `ocr-anywhere-setup-x86.exe` | Windows 10 32-bit |
| `ocr-anywhere-setup-arm64.exe` | Windows 11 on ARM |

Each release includes a `SHA256SUMS.txt` file to verify the download.

Everything is installed for the current user, without administrator rights:

| What | Where |
| :--- | :--- |
| Program | `%LOCALAPPDATA%\Programs\ocr-anywhere` |
| Settings and history | `%LOCALAPPDATA%\ocr-anywhere` |
| Start Menu | *OCR Anywhere* folder with shortcuts to the app and to the settings file |
| Start with Windows | On by default; turn it off in the **Settings** window |

Running the installer again offers **Update** or **Repair** (depending on the installed version) and **Uninstall**.

### Unattended installation

```powershell
.\ocr-anywhere-setup.exe --silent              # install, update or repair
.\ocr-anywhere-setup.exe --uninstall --silent  # uninstall
```

### Uninstall

Use *Settings → Apps → Installed apps → OCR Anywhere → Uninstall*, or run the installer again and choose **Uninstall**. The program, its shortcuts, its startup entry, settings and history are removed.

## Usage

| Action | How |
| :--- | :--- |
| Capture text | `Ctrl` + `Alt` + `O`, or **Capture text** in the tray menu |
| Select | Click and drag over the text; release to recognize |
| Cancel | `Esc`, right-click, or switch to another window |
| History | Double-click the tray icon, or **OCR History** in the tray menu |
| Settings | **Settings...** in the tray menu |

In the history window, double-click an entry (or press `Enter`) to copy it again.

## Configuration

Most options are available in the **Settings** window and apply immediately. They are stored in `%LOCALAPPDATA%\ocr-anywhere\config.ini`, which can also be edited by hand (restart OCR Anywhere afterwards).

| Section | Key | Values | Default | Description |
| :--- | :--- | :--- | :---: | :--- |
| `[general]` | `language` | `auto` / `en` / `es` | `auto` | Interface language |
| | `theme` | `system` / `light` / `dark` | `system` | Window theme |
| | `start_with_windows` | `true` / `false` | `true` | Start when you sign in |
| | `monitor_target` | `cursor` / `primary` / `0`, `1`, ... | `cursor` | Monitor to capture |
| `[hotkey]` | `modifiers` | combination of `ctrl`, `alt`, `shift`, `win` | `ctrl+alt` | Shortcut modifiers |
| | `key` | `A`–`Z`, `0`–`9`, `F1`–`F24` | `O` | Shortcut key |
| `[ocr]` | `language` | `auto` or a tag such as `en-US`, `es-ES` | `auto` | Recognition language |
| `[history]` | `enabled` | `true` / `false` | `true` | Keep an encrypted history |
| | `max_entries` | 1–10000 | `100` | Number of captures kept |

Letter and digit shortcuts must include `Ctrl`, `Alt` or `Win`, so the hotkey never blocks normal typing.

## Building from source

Requirements: [Rust](https://rustup.rs/) 1.82+ and the [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the *Desktop development with C++* workload.

```bash
git clone https://github.com/stickylfs/ocr-anywhere.git
cd ocr-anywhere
cargo test --workspace

# The installer embeds the application, so build it first:
cargo build --release -p ocr-anywhere
cargo build --release -p ocr-anywhere-installer
```

The installer is written to `target/release/ocr-anywhere-setup.exe` (the application alone is `target/release/ocr-anywhere.exe`). For other architectures add `--target i686-pc-windows-msvc` or `--target aarch64-pc-windows-msvc` to both commands.

## Project structure

```
ocr-anywhere/
├── assets/                        # Logo and application icon
├── crates/
│   ├── ocr-anywhere-common/       # config.ini model and parser, i18n strings
│   ├── ocr-anywhere/              # Tray application: capture, overlay, OCR, history, UI
│   └── ocr-anywhere-installer/    # ocr-anywhere-setup.exe (per-user installer/uninstaller)
├── .github/workflows/             # CI (lint, test, build) and release pipelines
├── CHANGELOG.md
├── CONTRIBUTING.md
├── SECURITY.md
├── THIRD-PARTY-NOTICES
└── LICENSE
```

## Privacy

OCR Anywhere has no network code and sends no telemetry. Screenshots are kept in memory only while you select and are never written to disk; the history is encrypted with your Windows account. See [SECURITY.md](SECURITY.md) for details and for how to report vulnerabilities.

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Credits

- Developed with [Google Antigravity](https://antigravity.google) and [Claude](https://claude.ai).
- [windows-rs](https://github.com/microsoft/windows-rs): Rust bindings for Win32 and WinRT (`Windows.Media.Ocr`).
- [serde](https://github.com/serde-rs/serde) and [serde_json](https://github.com/serde-rs/json): history serialization.

## License

Released under the [MIT License](LICENSE).
