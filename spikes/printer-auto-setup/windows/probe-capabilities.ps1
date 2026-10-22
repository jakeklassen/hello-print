# Spike probe (read-only): describe what the printer driver exposes.
#
# - Dumps the queue's PrintCapabilities XML (default context, and in the context
#   of each supplied DEVMODE blob).
# - Converts each DEVMODE blob to a Print Ticket so we can see which settings the
#   driver maps to named Print Schema features (vs. hidden private bytes).
# - Reports GetDeviceCaps page geometry for each DEVMODE (physical page vs.
#   printable area) as a cheap "is the page bigger than the sheet?" signal.
#
# Nothing is printed and no printer/queue settings are changed.
#
# Usage (Windows PowerShell 5.1, the same host Hello Print uses):
#   powershell -NoProfile -ExecutionPolicy Bypass -File probe-capabilities.ps1 `
#     -Queue "ET-8500 Series(Network)" -OutDir ..\..\..\tmp\spike\probe `
#     -DevModes ..\..\..\tmp\spike\golden\4x6.devmode.bin, ..\..\..\tmp\spike\golden\8.5x11.devmode.bin

param(
    [Parameter(Mandatory = $true)][string]$Queue,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [string[]]$DevModes = @()
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Printing, ReachFramework

# `-File` passes "a,b" as one string; accept both forms and resolve up front.
$DevModes = @($DevModes | ForEach-Object { $_ -split "," } | Where-Object { $_ } |
    ForEach-Object { (Resolve-Path $_).Path })

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class GdiProbe {
    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr CreateDC(string driver, string device, string output, IntPtr devMode);
    [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr hdc, int index);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr hdc);

    // index -> value for HORZSIZE, VERTSIZE, HORZRES, VERTRES, LOGPIXELSX/Y,
    // PHYSICALWIDTH/HEIGHT, PHYSICALOFFSETX/Y
    public static int[] Caps(string printer, byte[] devMode) {
        int[] idx = { 4, 6, 8, 10, 88, 90, 110, 111, 112, 113 };
        IntPtr dm = IntPtr.Zero;
        if (devMode != null) {
            dm = Marshal.AllocHGlobal(devMode.Length);
            Marshal.Copy(devMode, 0, dm, devMode.Length);
        }
        IntPtr hdc = CreateDC("WINSPOOL", printer, null, dm);
        if (dm != IntPtr.Zero) Marshal.FreeHGlobal(dm);
        if (hdc == IntPtr.Zero) throw new Exception("CreateDC failed: " + Marshal.GetLastWin32Error());
        int[] r = new int[idx.Length];
        for (int i = 0; i < idx.Length; i++) r[i] = GetDeviceCaps(hdc, idx[i]);
        DeleteDC(hdc);
        return r;
    }
}
"@

function Save-Stream($stream, $path) {
    $stream.Position = 0
    $fs = [System.IO.File]::Create($path)
    try { $stream.CopyTo($fs) } finally { $fs.Dispose() }
}

function Write-Geometry($label, [byte[]]$devMode) {
    $c = [GdiProbe]::Caps($Queue, $devMode)
    $dpiX = $c[4]; $dpiY = $c[5]
    $mm = { param($px, $dpi) [math]::Round($px / $dpi * 25.4, 2) }
    [pscustomobject]@{
        Context           = $label
        DpiX              = $dpiX
        DpiY              = $dpiY
        PhysicalWidthMm   = & $mm $c[6] $dpiX
        PhysicalHeightMm  = & $mm $c[7] $dpiY
        PrintableWidthMm  = & $mm $c[2] $dpiX
        PrintableHeightMm = & $mm $c[3] $dpiY
        OffsetXMm         = & $mm $c[8] $dpiX
        OffsetYMm         = & $mm $c[9] $dpiY
        HorzSizeMm        = $c[0]
        VertSizeMm        = $c[1]
    }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$server = New-Object System.Printing.LocalPrintServer
$pq = $server.GetPrintQueue($Queue)
$converter = New-Object System.Printing.Interop.PrintTicketConverter($Queue, $pq.ClientPrintSchemaVersion)

Save-Stream $pq.GetPrintCapabilitiesAsXml() (Join-Path $OutDir "capabilities.default.xml")
Save-Stream $pq.UserPrintTicket.GetXmlStream() (Join-Path $OutDir "ticket.user-default.xml")

$geometry = @(Write-Geometry "queue default" $null)

foreach ($path in $DevModes) {
    $name = [System.IO.Path]::GetFileNameWithoutExtension($path) -replace "\.devmode$", ""
    $bytes = [System.IO.File]::ReadAllBytes($path)

    $ticket = $converter.ConvertDevModeToPrintTicket($bytes)
    Save-Stream $ticket.GetXmlStream() (Join-Path $OutDir "ticket.$name.xml")
    Save-Stream $pq.GetPrintCapabilitiesAsXml($ticket) (Join-Path $OutDir "capabilities.$name.xml")

    $geometry += Write-Geometry $name $bytes
}

$geometry | Format-Table -AutoSize | Out-String -Width 200
Write-Output "Wrote XML to $((Resolve-Path $OutDir).Path)"
