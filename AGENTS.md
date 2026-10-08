# cheesestrap, for whoever works on this next

## Every change ships a version bump

There is no such thing as a change without a version bump. Not a bugfix, not a
typo in the readme, not a whitespace cleanup. Every single time, bump the patch
number.

Two places hold it, and they move together:

- `Cargo.toml` -> `version`
- `installer/Cheesestrap.iss` -> `#define MyAppVersion`

Nothing else needs touching. The setup filename is built from the define
(`OutputBaseFilename=Cheesestrap-Setup-{#MyAppVersion}-x64`), the release
workflow reads the version out of `Cargo.toml`, and the user agent reads
`CARGO_PKG_VERSION`. Do not hand write the number anywhere else.

## Every version gets changelog notes

Add a `### <version>` entry at the top of the Changelog section in `README.md`,
above the current one, saying what actually changed. The release workflow
copies that entry into the release notes verbatim, so whatever the readme
claims is what users get.

`installer\test-release-notes.ps1` checks the pairing locally:

```
powershell -File installer\test-release-notes.ps1
```

It exits non zero when the version in `Cargo.toml` has no entry, which is the
symptom of a bump without notes or notes without a bump.

## Shipping it

```
git push origin main
git tag -f release
git push --force origin release
```

The tag is literally `release`, it never moves forward on its own, so it gets
force updated each time. That push runs `.github/workflows/release.yml`, which
builds the app, builds the setup, reads the version and the notes, and
publishes the release. The workflow refuses to publish if the version did not
change since the last release, see the guard step.

## Building locally

```
cargo build --release
cd installer
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" /Qp Cheesestrap.iss
```

Needs a VS Build Tools prompt loaded, msvc only, see `rust-toolchain.toml`.
Inno Setup lives in `%LOCALAPPDATA%\Programs` for a per user install and in
`%ProgramFiles(x86)%` for the system one.

`installer\make-wizard-art.ps1` regenerates the wizard artwork from the logo,
run it only after changing `assets\branding\macncheese-512.png`.

## Testing an update

The self update path is easy to break without noticing, it needs two builds.
Publish a version, install it, then build the next one with the version
reverted one step and run it. It should show the update screen and, on
update, download, install and relaunch.