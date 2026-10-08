### Cheesestrap <img align="right" width="128" src="assets/branding/macncheese-256.png" alt="Cheesestrap logo" />

[![build](https://github.com/nenquen/cheesestrap/actions/workflows/build.yml/badge.svg)](https://github.com/nenquen/cheesestrap/actions/workflows/build.yml) [![license](https://img.shields.io/badge/license-GPL--3.0-green)](LICENSE)

A small roblox client bootstrapper. No official installer, the client is
downloaded into our own `clients/` folder and updated before every launch.

The interface is a ratatui TUI painted inside our own window through egui, so
there is no console and no terminal window behind it.

## Build

The toolchain is pinned in `rust-toolchain.toml`, msvc only.

```
powershell -ExecutionPolicy Bypass -File installer\build-release.ps1
```

That stamps the version, builds the exe and wraps it in a setup in one go. A
plain `cargo build --release` works too but then reports the crate version
instead of a build stamp.

The exe lands in `target/release/cheesestrap.exe` as a gui binary. The icon is
built at compile time from `assets/branding/macncheese-512.png`.

## Installer

Needs Inno Setup 6.

```
cd installer
powershell -ExecutionPolicy Bypass -File make-wizard-art.ps1
powershell -ExecutionPolicy Bypass -File stamp-version.ps1
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /Qp /DMyAppVersion=2026-10-08-1436 Cheesestrap.iss
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

`.github/workflows/release.yml` does the same and then publishes a github
release with the setup exe attached. Push the `release` tag to cut one, the
release notes come from `installer/notes.txt`.

## Contributors

<img src="https://contrib.rocks/image?repo=nenquen/cheesestrap" alt="Contributors" />

