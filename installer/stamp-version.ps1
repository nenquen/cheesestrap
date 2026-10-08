# Stamps a date based version and writes it where everything else reads it.
#
#   1.0.YYYYMMDD.HHMM   in UTC
#
# Fixed width parts, so a plain numeric compare sorts them chronologically and
# "is this newer" needs no special cases. UTC rather than local time so a build
# on a laptop and the one the runner produces are formatted the same way.
#
# The version lands in two places on purpose:
#   target\version.ini  read by the inno script
#   $env:CHEESESTRAP_VERSION  picked up by cargo for the compiled in binary
#
# Run: powershell -File installer\stamp-version.ps1
param([string]$Version = "")

$ErrorActionPreference = "Stop"

$now = [DateTime]::UtcNow
if (-not $Version) {
    $Version = "1.0.{0}{1:00}{2:00}.{3:00}{4:00}" -f `
        $now.Year, $now.Month, $now.Day, $now.Hour, $now.Minute
}

$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root "target"
New-Item -ItemType Directory -Path $target -Force | Out-Null

# ascii with no bom, inno's ReadIni chokes on a bom. needs the trailing
# newline or the last line is not parsed
$ini = Join-Path $target "version.ini"
"[Version]`r`nVersion=$Version`r`n" | Set-Content $ini -Encoding ascii

$env:CHEESESTRAP_VERSION = $Version
Write-Output "stamped $Version"