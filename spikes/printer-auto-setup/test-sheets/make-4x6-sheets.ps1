# Spike: generate the 4x6 measurement sheets (300 dpi, 1200 x 1800 px).
#
#   A - centre: a 50 x 50 mm square dead centre plus a 140 mm line. Measures size.
#   B - edges:  coloured background, a frame on the image edge, 20 mm squares in
#               every corner, and 0-10 mm rulers on each edge. Shows clipping (lost
#               ticks / frame) vs. shift (white strip) per edge.
#
# Every dimension is rounded to whole pixels (mm * 300 / 25.4).
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File make-4x6-sheets.ps1 `
#          -OutDir ..\..\..\tmp\spike\sheets [-Label "baseline Retain Size"]

param(
    [Parameter(Mandatory = $true)][string]$OutDir,
    [string]$Label = "baseline Retain Size"
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$W = 1200; $H = 1800
function Px([double]$mm) { [int][math]::Round($mm * 300 / 25.4) }

$OutDir = [System.IO.Path]::GetFullPath($OutDir)
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function New-Sheet([System.Drawing.Color]$background) {
    $bmp = New-Object System.Drawing.Bitmap($W, $H)
    $bmp.SetResolution(300, 300)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = "None"
    $g.TextRenderingHint = "AntiAliasGridFit"
    $g.Clear($background)
    return $bmp, $g
}

$black = [System.Drawing.Brushes]::Black
$font = New-Object System.Drawing.Font("Segoe UI", 7, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Point)
$small = New-Object System.Drawing.Font("Segoe UI", 5, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Point)

# --- Sheet A: centre square + 140 mm line -----------------------------------
$bmp, $g = New-Sheet ([System.Drawing.Color]::White)

$sq = Px 50                              # 591
$x0 = [int][math]::Floor(($W - $sq) / 2) # 304
$y0 = [int][math]::Floor(($H - $sq) / 2) # 604
$g.FillRectangle($black, $x0, $y0, $sq, $sq)
$g.DrawString("50 x 50 mm ($sq x $sq px)", $font, $black, $x0, $y0 + $sq + 12)

$line = Px 140                           # 1654
$ly = [int][math]::Floor(($H - $line) / 2)
$g.FillRectangle($black, 150, $ly, 4, $line)
$g.FillRectangle($black, 130, $ly, 44, 4)            # end caps mark the exact ends
$g.FillRectangle($black, 130, $ly + $line - 4, 44, 4)
$g.DrawString("140 mm ($line px)", $font, $black, 170, $ly + 20)

$g.DrawString("Sheet A - centre - $Label - 4x6 @ 300 dpi", $small, $black, 330, 1500)
$g.Dispose()
$bmp.Save((Join-Path $OutDir "spike-A-centre-50mm-4x6.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()

# --- Sheet B: edges ----------------------------------------------------------
$bmp, $g = New-Sheet ([System.Drawing.Color]::FromArgb(255, 190, 225, 245))

# Frame exactly on the image edge, 0.5 mm thick.
$t = Px 0.5
$g.FillRectangle($black, 0, 0, $W, $t)
$g.FillRectangle($black, 0, $H - $t, $W, $t)
$g.FillRectangle($black, 0, 0, $t, $H)
$g.FillRectangle($black, $W - $t, 0, $t, $H)

# 20 mm squares in each corner, touching both edges.
$corner = Px 20                          # 236
$dark = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 30, 60, 110))
foreach ($c in @(@(0, 0), @(($W - $corner), 0), @(0, ($H - $corner)), @(($W - $corner), ($H - $corner)))) {
    $g.FillRectangle($dark, $c[0], $c[1], $corner, $corner)
}

# 0-10 mm rulers at the middle of each edge: tick every mm, long tick at 5 and 10.
$midX = [int]($W / 2); $midY = [int]($H / 2)
for ($mm = 1; $mm -le 10; $mm++) {
    $p = Px $mm
    $len = if ($mm % 5 -eq 0) { 80 } else { 45 }
    $g.FillRectangle($black, $p - 1, $midY - $len / 2, 3, $len)            # left edge
    $g.FillRectangle($black, $W - $p - 2, $midY - $len / 2, 3, $len)       # right edge
    $g.FillRectangle($black, $midX - $len / 2, $p - 1, $len, 3)            # top edge
    $g.FillRectangle($black, $midX - $len / 2, $H - $p - 2, $len, 3)       # bottom edge
}
$g.DrawString("L 1-10mm", $small, $black, 20, $midY + 50)
$g.DrawString("R 1-10mm", $small, $black, $W - 160, $midY + 50)
$g.DrawString("T 1-10mm", $small, $black, $midX + 50, 20)
$g.DrawString("B 1-10mm", $small, $black, $midX + 50, $H - 70)

$g.FillRectangle($black, $x0, $y0, $sq, $sq)
$g.DrawString("centre 50 x 50 mm; corner squares 20 x 20 mm", $font, $black, $x0 - 120, $y0 + $sq + 12)
$g.DrawString("Sheet B - edges - $Label - 4x6 @ 300 dpi", $small, $black, 330, 1450)
$g.Dispose()
$bmp.Save((Join-Path $OutDir "spike-B-edges-4x6.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()

Write-Output "Wrote sheets to $OutDir (square $sq px at $x0,$y0; line $line px)"
