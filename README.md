# Cheesestrap <img align="right" width="128" src="assets/branding/macncheese-256.png" alt="Cheesestrap logo" />

[![build](https://github.com/nenquen/cheesestrap/actions/workflows/build.yml/badge.svg)](https://github.com/nenquen/cheesestrap/actions/workflows/build.yml) [![license](https://img.shields.io/badge/license-GPL--3.0-green)](LICENSE)

A small roblox client bootstrapper. No official installer, the client is
downloaded into our own `clients/` folder and updated before every launch.

The interface is a ratatui TUI painted inside our own window through egui, so
there is no console and no terminal window behind it.

## Build

The toolchain is pinned in `rust-toolchain.toml`, msvc only.

```
cargo build --release
```

The exe lands in `target/release/cheesestrap.exe` as a gui binary. The icon is
built at compile time from `assets/branding/macncheese-512.png`.

## Installer

Needs Inno Setup 6.

```
cd installer
powershell -ExecutionPolicy Bypass -File make-wizard-art.ps1
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" Cheesestrap.iss
```

`make-wizard-art.ps1` regenerates `wizard.bmp` and `wizard-small.bmp` from the
branding logo, run it after changing the logo. The setup lands in
`installer/output/`.

## Where things go

Everything lives next to the exe, so under `C:\Program Files\Cheesestrap`:

- `clients/` downloaded roblox builds, one folder per version guid
- `logs/` log files, only written when the setting is on
- `settings.json`

## CI

`.github/workflows/build.yml` builds the app and then the setup exe on every
push, and uploads both as artifacts.