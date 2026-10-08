# Local dry run of the changelog scraping the release workflow does, so a
# missing entry is caught before pushing the release tag.
# Run: powershell -File installer\test-release-notes.ps1 [-Version 1.0.2]
param([string]$Version = "")

if (-not $Version) {
    $toml = Get-Content (Join-Path $PSScriptRoot "..\Cargo.toml") -Raw
    if ($toml -notmatch '(?m)^version\s*=\s*"([^"]+)"') { throw "no version in Cargo.toml" }
    $Version = $Matches[1]
}

$lines = Get-Content (Join-Path $PSScriptRoot "..\README.md")
$body = @()
$inside = $false
$found = $false
foreach ($line in $lines) {
    if ($line -match '^(#{2,3})\s+(.*)$') {
        $level = $Matches[1].Length
        $heading = $Matches[2].Trim()
        # a new heading ends the entry, anything shallower ends the changelog
        if ($inside -and $level -le 3) { break }
        if (-not $inside -and $level -eq 3 -and $heading -eq $Version) {
            $inside = $true
            $found = $true
            continue
        }
        continue
    }
    if ($inside) { $body += $line }
}

if (-not $found) {
    Write-Error "no changelog entry for $Version, the release would ship without notes"
    exit 1
}

$notes = ($body -join "`n").Trim()
Write-Output "version: $Version"
Write-Output "notes:"
Write-Output $notes