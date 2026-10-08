# Local dry run of the changelog scraping the release workflow does, so a
# missing entry is caught before pushing the release tag.
#
# The version is generated at build time now, so this cannot look for a heading
# that matches it. It takes the newest entry under ## Changelog, same as the
# workflow does.
#
# Run: powershell -File installer\test-release-notes.ps1

$ErrorActionPreference = "Stop"

$readme = Join-Path $PSScriptRoot "..\README.md"
$lines = Get-Content $readme

$start = ($lines | Select-String '^##\s+Changelog\s*$' | Select-Object -First 1).LineNumber
if (-not $start) { Write-Error "no ## Changelog heading in README.md"; exit 1 }

$body = @()
$inside = $false
$heading = ""
foreach ($line in $lines[$start..($lines.Count - 1)]) {
    if ($line -match '^##\s') { break }
    if ($line -match '^###\s') {
        if ($inside) { break }        # second entry, we only wanted the first
        $inside = $true
        $heading = $line.Substring(3).Trim()
        continue
    }
    if ($inside) { $body += $line }
}

$notes = ($body -join "`n").Trim()
if (-not $notes) {
    Write-Error "the newest changelog entry under ## Changelog is empty"
    exit 1
}

$stamp = Join-Path $PSScriptRoot "..\target\version.ini"
if (Test-Path $stamp) {
    $v = ((Get-Content $stamp | Select-String '^Version=').Line).Split('=')[1].Trim()
    Write-Output "this build: $v"
}
Write-Output "entry: $heading"
Write-Output "notes:"
Write-Output $notes