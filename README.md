<div align="right">
  <img src="assets/branding/macncheese-256.png" width="112" alt="Cheesestrap">
</div>

# Cheesestrap

![build](https://github.com/nenquen/cheesestrap/actions/workflows/build.yml/badge.svg)
![license](https://img.shields.io/badge/license-GPL--3.0-green)
![rust](https://img.shields.io/badge/rust-stable-2024%20edition-orange)
![platform](https://img.shields.io/badge/platform-windows-x64-0078D4)

A small roblox client bootstrapper. No official installer, the client is
downloaded into our own `clients/` folder and updated before every launch.

The interface is a ratatui TUI painted inside our own window through egui, so
there is no console and no terminal window behind it.

## build

The toolchain is pinned in `rust-toolchain.toml`, msvc only.

```
cargo build --release
```

The exe lands in `target/release/cheesestrap.exe` as a gui binary. The icon is
built at compile time from `assets/branding/macncheese-512.png`.

## installer

Needs Inno Setup 6.

```
cd installer
powershell -ExecutionPolicy Bypass -File make-wizard-art.ps1
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" Cheesestrap.iss
```

`make-wizard-art.ps1` regenerates `wizard.bmp` and `wizard-small.bmp` from the
branding logo, run it after changing the logo. The setup lands in
`installer/output/`.

## where things go

Everything lives next to the exe, so under `C:\Program Files\Cheesestrap`:

- `clients/` downloaded roblox builds, one folder per version guid
- `logs/` log files, only written when the setting is on
- `settings.json`

## ci

`.github/workflows/build.yml` builds the app and then the setup exe on every
push, and uploads both as artifacts.