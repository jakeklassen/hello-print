# Spike prototype: build a 1:1 printer setup with no driver dialog.
#
#   1. Start from the queue's default DEVMODE.
#   2. Set the named settings (size, borderless, source, paper type, quality,
#      orientation, colour) through a Print Ticket, merged and validated by the
#      driver and converted back to a DEVMODE by the driver.
#   3. ET-8500 family only: set the Retain Size flag byte, then let the driver
#      validate the blob again (DocumentProperties DM_IN_BUFFER | DM_OUT_BUFFER).
#   4. Verify, never trust: read the result back and check
#        - the requested Print Ticket options are what the driver reports, and
#        - the page geometry says "no expansion": the logical dpi equals the
#          print quality dpi (Auto Expand reports a higher dpi, e.g. 758 vs 720).
#
# Nothing is printed and the queue's defaults are not changed.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File build-profile.ps1 `
#     -Queue "HelloPrint Spike ET-8500" -Out ..\..\..\tmp\spike\built\4x6.bin `
#     [-MediaSize ns0000:Fullsize4x6] [-PaperType psk:Bond] [-Source ns0000:AutoSheetFeeder] `
#     [-Quality ns0000:HighQuality] [-NoRetainSize]

param(
    [Parameter(Mandatory = $true)][string]$Queue,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$MediaSize = "ns0000:Fullsize4x6",
    [string]$PaperType = "psk:Bond",                 # "Presentation Paper Matte"
    [string]$Source = "ns0000:AutoSheetFeeder",      # "Rear Paper Feeder"
    [string]$Quality = "ns0000:HighQuality",
    [switch]$NoRetainSize
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Printing, ReachFramework

# ET-8500 driver 0x300 / dmDriverExtra 7972: Retain Size flag (1 = Retain Size,
# 0 = Auto Expand), found by diffing dialog captures that differ only in that
# setting. Only valid for that exact layout; the guard below refuses otherwise.
$RetainSizeOffset = 7942
$ExpectedDriverVersion = 0x300
$ExpectedDriverExtra = 7972

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class Driver {
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool OpenPrinter(string name, out IntPtr hPrinter, IntPtr defaults);
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern int DocumentProperties(IntPtr hWnd, IntPtr hPrinter, string device, IntPtr output, IntPtr input, int mode);
    [DllImport("winspool.drv")] static extern bool ClosePrinter(IntPtr hPrinter);
    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr CreateDC(string driver, string device, string output, IntPtr devMode);
    [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr hdc, int index);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr hdc);

    // DocumentProperties(DM_IN_BUFFER | DM_OUT_BUFFER): the driver validates the
    // blob and returns its own version of it.
    public static byte[] Validate(string printer, byte[] devMode) {
        IntPtr h;
        if (!OpenPrinter(printer, out h, IntPtr.Zero)) throw new Exception("OpenPrinter failed");
        try {
            int size = DocumentProperties(IntPtr.Zero, h, printer, IntPtr.Zero, IntPtr.Zero, 0);
            IntPtr buf = Marshal.AllocHGlobal(Math.Max(size, devMode.Length));
            try {
                Marshal.Copy(devMode, 0, buf, Math.Min(size, devMode.Length));
                int ret = DocumentProperties(IntPtr.Zero, h, printer, buf, buf, 8 | 2);
                if (ret != 1) throw new Exception("Driver rejected DEVMODE: " + ret);
                byte[] result = new byte[size];
                Marshal.Copy(buf, result, 0, size);
                return result;
            } finally { Marshal.FreeHGlobal(buf); }
        } finally { ClosePrinter(h); }
    }

    // LOGPIXELSX, PHYSICALWIDTH, PHYSICALHEIGHT
    public static int[] Geometry(string printer, byte[] devMode) {
        IntPtr dm = Marshal.AllocHGlobal(devMode.Length);
        Marshal.Copy(devMode, 0, dm, devMode.Length);
        IntPtr hdc = CreateDC("WINSPOOL", printer, null, dm);
        Marshal.FreeHGlobal(dm);
        if (hdc == IntPtr.Zero) throw new Exception("CreateDC failed");
        int[] r = { GetDeviceCaps(hdc, 88), GetDeviceCaps(hdc, 110), GetDeviceCaps(hdc, 111) };
        DeleteDC(hdc);
        return r;
    }
}
"@

$psf = "http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework"
$psk = "http://schemas.microsoft.com/windows/2003/08/printing/printschemakeywords"
$epson = "http://schema.epson.net/printschema/business/v100"

$requested = [ordered]@{
    "psk:PageMediaSize"     = $MediaSize
    "psk:PageBorderless"    = "psk:Borderless"
    "psk:PageInputBin"      = $Source
    "psk:PageMediaType"     = $PaperType
    "psk:PageOutputQuality" = $Quality
    "psk:PageOrientation"   = "psk:Portrait"
    "psk:PageOutputColor"   = "psk:Color"
}

$server = New-Object System.Printing.LocalPrintServer
$pq = $server.GetPrintQueue($Queue)
$converter = New-Object System.Printing.Interop.PrintTicketConverter($Queue, $pq.ClientPrintSchemaVersion)

# A media size option is only honoured with its width/height scored properties,
# so copy them from the driver's own PrintCapabilities.
$caps = New-Object System.Xml.XmlDocument
$caps.Load($pq.GetPrintCapabilitiesAsXml())
$capsNs = New-Object System.Xml.XmlNamespaceManager($caps.NameTable)
$capsNs.AddNamespace("psf", $psf)
$sizeOption = $caps.SelectSingleNode("//psf:Feature[@name='psk:PageMediaSize']/psf:Option[@name='$MediaSize']", $capsNs)
if (-not $sizeOption) { throw "Driver has no media size '$MediaSize'" }
$sizeProps = foreach ($dim in "psk:MediaSizeWidth", "psk:MediaSizeHeight") {
    $v = $sizeOption.SelectSingleNode("psf:ScoredProperty[@name='$dim']/psf:Value", $capsNs).InnerText
    "<psf:ScoredProperty name=`"$dim`"><psf:Value xsi:type=`"xsd:integer`">$v</psf:Value></psf:ScoredProperty>"
}

$features = ($requested.GetEnumerator() | ForEach-Object {
    $inner = if ($_.Key -eq "psk:PageMediaSize") { $sizeProps -join "" } else { "" }
    "  <psf:Feature name=`"$($_.Key)`"><psf:Option name=`"$($_.Value)`">$inner</psf:Option></psf:Feature>"
}) -join "`n"
$deltaXml = @"
<?xml version="1.0" encoding="UTF-8"?>
<psf:PrintTicket xmlns:psf="$psf" xmlns:psk="$psk" xmlns:ns0000="$epson"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" version="1">
$features
</psf:PrintTicket>
"@

# 1-2. Queue default + named settings, merged and converted by the driver.
$base = $pq.DefaultPrintTicket
$delta = New-Object System.Printing.PrintTicket(New-Object System.IO.MemoryStream(, [System.Text.Encoding]::UTF8.GetBytes($deltaXml)))
$merged = $pq.MergeAndValidatePrintTicket($base, $delta).ValidatedPrintTicket
$devMode = $converter.ConvertPrintTicketToDevMode($merged, [System.Printing.Interop.BaseDevModeType]::UserDefault)

# 3. Retain Size, guarded by the layout the offset was found in.
$driverVersion = [BitConverter]::ToUInt16($devMode, 66)
$driverExtra = [BitConverter]::ToUInt16($devMode, 70)
$layoutKnown = $driverVersion -eq $ExpectedDriverVersion -and $driverExtra -eq $ExpectedDriverExtra -and $devMode.Length -gt $RetainSizeOffset
if (-not $NoRetainSize) {
    if (-not $layoutKnown) {
        throw ("Unknown driver layout (version 0x{0:x}, extra {1}); refusing to patch Retain Size" -f $driverVersion, $driverExtra)
    }
    $devMode[$RetainSizeOffset] = 1
}
$devMode = [Driver]::Validate($Queue, $devMode)

# 4. Verify.
$check = $converter.ConvertDevModeToPrintTicket($devMode)
$xml = New-Object System.Xml.XmlDocument
$xml.Load($check.GetXmlStream())
$ns = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
$ns.AddNamespace("psf", $psf)

$report = foreach ($kv in $requested.GetEnumerator()) {
    $actual = $xml.SelectSingleNode("//psf:Feature[@name='$($kv.Key)']/psf:Option/@name", $ns)
    [pscustomobject]@{ Setting = $kv.Key; Requested = $kv.Value; Driver = $actual.Value; Ok = ($actual.Value -eq $kv.Value) }
}

$quality = [BitConverter]::ToInt16($devMode, 90)
$geo = [Driver]::Geometry($Queue, $devMode)
$retainFlag = if ($layoutKnown) { $devMode[$RetainSizeOffset] } else { "n/a" }
$noExpansion = $geo[0] -eq $quality

$report | Format-Table -AutoSize | Out-String -Width 200
Write-Output ("Retain Size flag: {0}; logical dpi {1} vs print quality {2} dpi -> {3}; page {4:N2} x {5:N2} mm" -f `
    $retainFlag, $geo[0], $quality, $(if ($noExpansion) { "NO expansion" } else { "EXPANDING x{0:N4}" -f ($geo[0] / $quality) }), `
    ($geo[1] / $geo[0] * 25.4), ($geo[2] / $geo[0] * 25.4))

$Out = [System.IO.Path]::GetFullPath($Out)
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
[System.IO.File]::WriteAllBytes($Out, $devMode)
Write-Output "Wrote $Out"

if (($report | Where-Object { -not $_.Ok }) -or (-not $NoRetainSize -and -not $noExpansion)) { exit 2 }
