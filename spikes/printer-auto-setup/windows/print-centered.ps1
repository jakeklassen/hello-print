# Spike prototype: Hello Print's Windows print path, but centring the image on
# the physical sheet instead of drawing it at the page origin.
#
# With Epson "Retain Size", the driver's page is larger than the paper (bleed
# room on every side) and the paper sits in the middle of it. Hello Print today
# draws the image at (0,0) with the paper's size, so prints come out shifted up
# and left. This draws at
#   ((physicalPage - paper) / 2 - physicalOffset)
# per axis, which is a no-op for drivers whose page equals the paper.
#
# -DryRun prints the geometry and the offset it would apply, without printing.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File print-centered.ps1 `
#     -Queue "HelloPrint Spike ET-8500" -DevMode ..\..\..\tmp\spike\golden\4x6.devmode.bin `
#     -Image ..\..\..\tmp\spike\sheets\spike-B-edges-4x6.png [-DryRun]

param(
    [Parameter(Mandatory = $true)][string]$Queue,
    [Parameter(Mandatory = $true)][string]$DevMode,
    [Parameter(Mandatory = $true)][string]$Image,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

Add-Type -TypeDefinition @"
using System;
using System.Drawing.Printing;
using System.Runtime.InteropServices;

public static class CenteredPrint {
    [DllImport("kernel32.dll")] static extern IntPtr GlobalAlloc(uint flags, UIntPtr size);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr hMem);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr hMem);
    [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr hdc, int index);

    // Same as Hello Print's DevModeApplier.
    public static void Apply(PrinterSettings ps, PageSettings page, byte[] dm) {
        IntPtr hGlobal = GlobalAlloc(0x0042, (UIntPtr)dm.Length);
        IntPtr ptr = GlobalLock(hGlobal);
        Marshal.Copy(dm, 0, ptr, dm.Length);
        GlobalUnlock(hGlobal);
        ps.SetHdevmode(hGlobal);
        page.SetHdevmode(hGlobal);
    }

    // PHYSICALWIDTH, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY, LOGPIXELSX, LOGPIXELSY
    public static int[] Physical(IntPtr hdc) {
        int[] idx = { 110, 111, 112, 113, 88, 90 };
        int[] r = new int[idx.Length];
        for (int i = 0; i < idx.Length; i++) r[i] = GetDeviceCaps(hdc, idx[i]);
        return r;
    }
}
"@ -ReferencedAssemblies System.Drawing

$devModeBytes = [System.IO.File]::ReadAllBytes((Resolve-Path $DevMode))
$img = [System.Drawing.Image]::FromFile((Resolve-Path $Image).Path)

$pd = New-Object System.Drawing.Printing.PrintDocument
$pd.PrinterSettings.PrinterName = $Queue
$pd.DocumentName = "HelloPrint Spike - $([System.IO.Path]::GetFileName($Image))"
[CenteredPrint]::Apply($pd.PrinterSettings, $pd.DefaultPageSettings, $devModeBytes)

# Offset in the Graphics' units (1/100 inch, origin at the printable-area
# corner), computed from the device's physical page vs. the paper size.
function Get-Offset($graphics, $pageBounds) {
    $hdc = $graphics.GetHdc()
    try { $c = [CenteredPrint]::Physical($hdc) } finally { $graphics.ReleaseHdc($hdc) }
    $physW = $c[0] / $c[4] * 100; $physH = $c[1] / $c[5] * 100
    $offX = $c[2] / $c[4] * 100;  $offY = $c[3] / $c[5] * 100
    [pscustomobject]@{
        PhysicalMm = "{0:N2} x {1:N2}" -f ($physW * 0.254), ($physH * 0.254)
        PaperMm    = "{0:N2} x {1:N2}" -f ($pageBounds.Width * 0.254), ($pageBounds.Height * 0.254)
        X          = ($physW - $pageBounds.Width) / 2 - $offX
        Y          = ($physH - $pageBounds.Height) / 2 - $offY
    }
}

if ($DryRun) {
    $g = $pd.PrinterSettings.CreateMeasurementGraphics($pd.DefaultPageSettings)
    $o = Get-Offset $g $pd.DefaultPageSettings.Bounds
    $g.Dispose()
    Write-Output ("Physical page {0} mm, paper {1} mm -> draw image at +{2:N2} mm, +{3:N2} mm" -f `
        $o.PhysicalMm, $o.PaperMm, ($o.X * 0.254), ($o.Y * 0.254))
    $img.Dispose()
    return
}

$pd.add_PrintPage({
    param($sender, $e)
    $o = Get-Offset $e.Graphics $e.PageBounds
    Write-Output ("Printing: physical {0} mm, paper {1} mm, offset +{2:N2} mm, +{3:N2} mm" -f `
        $o.PhysicalMm, $o.PaperMm, ($o.X * 0.254), ($o.Y * 0.254))
    $dest = New-Object System.Drawing.RectangleF([single]$o.X, [single]$o.Y, $e.PageBounds.Width, $e.PageBounds.Height)
    $e.Graphics.DrawImage($img, $dest)
    $e.HasMorePages = $false
})

$pd.Print()
$img.Dispose()
$pd.Dispose()
Write-Output "Sent to $Queue"
