# cheesestrap

A small roblox client bootstrapper. No installer-based roblox, the client gets
downloaded into our own `clients/` folder, updated before every launch.

The interface is a ratatui TUI painted inside our own window through egui, so
there is no console and no terminal window behind it.

## build

Needs a stable msvc toolchain, which `rust-toolchain.toml` pins for you.

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

Everything lives next to the exe, which means under `C:\Program Files\Cheesestrap`:

- `clients/` downloaded roblox builds, one folder per version guid
- `logs/` log files, only written when the setting is on
- `settings.json`

## ci

`.github/workflows/build.yml` builds the app and then the setup exe on every
push, and uploads both as artifacts.