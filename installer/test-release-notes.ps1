# Prints what the release workflow would publish and exits non zero when the
# notes are missing or empty, so a release cannot go out silent.
#
# There is no history on purpose. installer\notes.txt holds the notes for the
# build that is about to ship and gets rewritten each time, nothing accumulates.
#
# Run: powershell -File installer\test-release-notes.ps1

$ErrorActionPreference = "Stop"

$file = Join-Path $PSScriptRoot "notes.txt"
if (-not (Test-Path $file)) { Write-Error "$file is missing"; exit 1 }

$notes = (Get-Content $file -Raw).Trim()
if (-not $notes) { Write-Error "$file is empty"; exit 1 }

$stamp = Join-Path $PSScriptRoot "..\target\version.ini"
if (Test-Path $stamp) {
    $v = ((Get-Content $stamp | Select-String '^Version=').Line).Split('=')[1].Trim()
    Write-Output "this build: $v"
}
Write-Output "---"
Write-Output $notes