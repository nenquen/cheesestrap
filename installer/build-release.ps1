# Full local release build: stamp, compile, wrap in a setup.
#
# Run from anywhere:
#   powershell -ExecutionPolicy Bypass -File installer\build-release.ps1
#
# The version is generated here, see stamp-version.ps1.

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$stamp = Join-Path $PSScriptRoot "stamp-version.ps1"

Write-Host "== stamping version =="
& $stamp

Write-Host "== building the app =="
Push-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# $env is process wide, so the stamp set above is already visible to cargo here
if (-not $env:CHEESESTRAP_VERSION) { Pop-Location; throw "stamp did not set a version" }
cargo build --release --locked
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "cargo build failed" }

Write-Host "== building the setup =="
$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $iscc)) {
    $iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
}
if (-not (Test-Path $iscc)) { Pop-Location; throw "ISCC.exe not found" }

# every build is its own version, so old setups are dead weight here
$out = Join-Path $PSScriptRoot "output"
Remove-Item (Join-Path $out "*.exe") -Force -ErrorAction SilentlyContinue

& $iscc /Qp "/DMyAppVersion=$env:CHEESESTRAP_VERSION" (Join-Path $PSScriptRoot "Cheesestrap.iss")
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "ISCC failed" }
Pop-Location

Get-ChildItem (Join-Path $PSScriptRoot "output") -Filter "*.exe" |
    ForEach-Object { Write-Host "built $($_.Name) $([math]::Round($_.Length / 1MB, 1))MB" }