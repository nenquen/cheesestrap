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
notes come from the top entry of the changelog below.

## Changelog

Versions are the moment the build happened, `2026-10-08-1436` in UTC, shown
inside the app as `08-10-2026 14:36`. Nothing to bump, every build is its own
version and the update check just compares the numbers.

### date based versions

- the update check cache is 30 minutes instead of 6 hours. six hours meant a
  release could sit invisible all afternoon, and a user whose cache was fresh
  could not see the very update that fixed whatever the cache was hiding
- a failed check now counts as a check too, otherwise every launch hammers a
  github that already said no
- the github rate limit headers are read back and logged when they run low, a
  silent 403 is how a version check quietly dies
- the version is the build time, no hand written number is left anywhere.
  `installer\stamp-version.ps1` computes it and the inno script and the binary
  both get it from there
- the app writes the date day first, `08-10-2026 14:36`. the file name stays
  iso on purpose, a day first stamp would make every already installed build
  read the newest release as older and stop offering updates
- unit tests cover the comparison, the display format and pulling the stamp out
  of the setup file name
- release notes take the newest changelog entry instead of matching a version,
  since the version is not known until the build starts
- the update cache records which build wrote it, so an app that was just
  installed always looks again instead of inheriting the previous version's
  timestamp and reporting up to date without ever asking github

### 1.0.9

- the update check is cached for six hours, github only allows 60 requests an
  hour per ip and asking on every launch silently starved users of updates
- reads the release list and takes the highest version instead of trusting
  /releases/latest, which orders by created_at and we always amend the same one
- draft and prerelease entries are skipped
- a short or failed download is deleted instead of leaving a broken setup behind,
  and a truncated one is refused rather than run
- a loose exe that was never installed skips the check instead of installing a
  second copy under program files
- connect timeout so a black hole network cannot hang the check forever

### 1.0.8

- the update button was one row below where it was drawn, so clicking it did
  nothing and the only way through was the enter key

### 1.0.7

- the status box shows the running cheesestrap version

### 1.0.6

- cleanup pass, no behaviour change
- the http client and the file downloader existed twice, once for roblox and
  once for the updater, they live in `net.rs` now with one connection pool
- dropped twenty unused colour arms in `theme.rs` and an empty
  `.cargo/config.toml` left over from the mingw days
- rewrote the log wrapping, it was doing a hand rolled prepend per line
- clippy is clean
- AGENTS.md spells out the bump and changelog rules for whoever works on this next

### 1.0.4

- updates are mandatory now, there is no later and no way past it
- the update screen just says what arrived and what you are on, nothing more

### 1.0.3

- the update screen owns the whole window while it is up, no panels peeking
  out from behind it
- the app relaunches itself once the update finished installing, the silent
  setup was swallowing the postinstall launch so it just stayed closed

### 1.0.2

- self update: the app checks the github releases api on launch and shows a
  notice when a newer version exists, pressing update downloads the setup and
  reinstalls over the running app
- release workflow: pushing the `release` tag builds the setup and publishes
  it as a release with the matching changelog entry as the notes
- webview2 detection reads the 32 bit registry view, which is where the x64
  runtime actually registers itself, it was reporting a healthy install as
  missing

### 1.0.1

- webview2 repair: replaces the old delete webview2 setting, which left a
  stale edge registration behind and made roblox pop an install dialog with
  nothing to install from
- setup and exe keep all their data next to the exe instead of appdata
- desktop shortcut no longer carries x64 in its name

### 1.0.0

- first public build
