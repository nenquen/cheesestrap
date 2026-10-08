# Regenerates the Inno Setup wizard artwork from the real app logo.
#   wizard.bmp       164x314, shown down the left side of every setup page
#   wizard-small.bmp  55x55, shown on the taskbar / Alt+Tab while installing
# Both are 24bpp opaque BMPs so Inno never has to guess at an alpha channel.
# Nothing is drawn here: the cheese comes straight from the branding PNG, only
# centred on the chocolate backdrop. The logo PNG already has transparent
# corners, so it composites cleanly over the gradient.

Add-Type -AssemblyName System.Drawing

$bgTop = [System.Drawing.Color]::FromArgb(255, 58, 39, 22)   # #3A2716
$bgBot = [System.Drawing.Color]::FromArgb(255, 34, 21, 10)   # #22150A

function New-Canvas([int]$w, [int]$h) {
    $bmp = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    return @($bmp, $g)
}

function Fill-VerticalGradient($g, [int]$w, [int]$h) {
    $rect = New-Object System.Drawing.Rectangle(0, 0, $w, $h)
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $rect, $bgTop, $bgBot, [System.Drawing.Drawing2D.LinearGradientMode]::Vertical)
    $g.FillRectangle($brush, $rect)
    $brush.Dispose()
}

# Draws $logo scaled to fit inside a $box square, centred on the canvas both
# ways. Centring on (size - 1) / 2 rather than size / 2 puts it on the pixel
# grid's mirror axis, so the result is symmetric when flipped.
function Draw-CenteredLogo($g, $logo, [int]$w, [int]$h, [int]$box) {
    $side = [Math]::Min($logo.Width, $logo.Height)
    $scale = $box / $side
    $dw = $logo.Width * $scale
    $dh = $logo.Height * $scale
    $cx = ($w - 1) / 2
    $cy = ($h - 1) / 2
    $dest = New-Object System.Drawing.RectangleF(
        [float]($cx - $dw / 2), [float]($cy - $dh / 2), [float]$dw, [float]$dh)
    $g.DrawImage($logo, $dest)
}

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$logo = [System.Drawing.Image]::FromFile((Join-Path $here "..\assets\branding\macncheese-512.png"))

# ---------------------------------------------------------------- wizard.bmp
$W = 164
$H = 314
$can = New-Canvas $W $H
$bmp = $can[0]; $g = $can[1]
Fill-VerticalGradient $g $W $H
Draw-CenteredLogo $g $logo $W $H 128
$g.Dispose()
$bmp.Save((Join-Path $here "wizard.bmp"), [System.Drawing.Imaging.ImageFormat]::Bmp)
$bmp.Dispose()
Write-Output "wizard.bmp       164x314"

# --------------------------------------------------------- wizard-small.bmp
$S = 55
$can = New-Canvas $S $S
$bmp = $can[0]; $g = $can[1]
Fill-VerticalGradient $g $S $S
Draw-CenteredLogo $g $logo $S $S 46
$g.Dispose()
$bmp.Save((Join-Path $here "wizard-small.bmp"), [System.Drawing.Imaging.ImageFormat]::Bmp)
$bmp.Dispose()
Write-Output "wizard-small.bmp  55x55"

$logo.Dispose()