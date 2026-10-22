# Spike probe: capture a DEVMODE from the driver's own dialog, or validate a
# saved one against a queue without any dialog.
#
# Starts from the queue's default DEVMODE, or from -Seed (a saved blob, e.g.
# from another queue). The seed is first merged with
# DocumentProperties(DM_IN_BUFFER | DM_OUT_BUFFER), so the driver validates it
# exactly the way Hello Print's replay does. The validated blob is written to
# <Out>.validated.bin. Unless -NoPrompt, the driver dialog then opens; change
# ONE setting, click OK, and the result is written to <Out>.
#
# This only reads/writes per-document settings. It does not change the queue's
# printing defaults. (Saving an Epson *preset* from inside the dialog would, so
# don't do that unless the step says to.)
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File capture-devmode.ps1 `
#     -Queue "HelloPrint Spike ET-8500" -Out ..\..\..\tmp\spike\captures\auto-expand.bin `
#     [-Seed ..\..\..\tmp\spike\golden\4x6.devmode.bin] [-NoPrompt]

param(
    [Parameter(Mandatory = $true)][string]$Queue,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Seed,
    [switch]$NoPrompt
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class DevModeDialog {
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool OpenPrinter(string name, out IntPtr hPrinter, IntPtr defaults);
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern int DocumentProperties(IntPtr hWnd, IntPtr hPrinter, string device,
        IntPtr output, IntPtr input, int mode);
    [DllImport("winspool.drv")] public static extern bool ClosePrinter(IntPtr hPrinter);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();

    public const int DM_OUT_BUFFER = 2, DM_IN_PROMPT = 4, DM_IN_BUFFER = 8, IDOK = 1;
}
"@

$Out = [System.IO.Path]::GetFullPath($Out)
New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null

$hPrinter = [IntPtr]::Zero
if (-not [DevModeDialog]::OpenPrinter($Queue, [ref]$hPrinter, [IntPtr]::Zero)) {
    throw "OpenPrinter failed for '$Queue'"
}

try {
    $size = [DevModeDialog]::DocumentProperties([IntPtr]::Zero, $hPrinter, $Queue, [IntPtr]::Zero, [IntPtr]::Zero, 0)
    $buf = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($size)

    try {
        [void][DevModeDialog]::DocumentProperties([IntPtr]::Zero, $hPrinter, $Queue, $buf, [IntPtr]::Zero, [DevModeDialog]::DM_OUT_BUFFER)

        if ($Seed) {
            $seedBytes = [System.IO.File]::ReadAllBytes((Resolve-Path $Seed))
            if ($seedBytes.Length -ne $size) {
                Write-Warning "Seed is $($seedBytes.Length) bytes, driver expects $size; copying the overlap."
            }
            [System.Runtime.InteropServices.Marshal]::Copy($seedBytes, 0, $buf, [math]::Min($size, $seedBytes.Length))
            $ret = [DevModeDialog]::DocumentProperties([IntPtr]::Zero, $hPrinter, $Queue, $buf, $buf,
                [DevModeDialog]::DM_IN_BUFFER -bor [DevModeDialog]::DM_OUT_BUFFER)
            if ($ret -ne [DevModeDialog]::IDOK) { throw "Driver rejected the seed (DocumentProperties returned $ret)" }
        }

        $validated = New-Object byte[] $size
        [System.Runtime.InteropServices.Marshal]::Copy($buf, $validated, 0, $size)
        [System.IO.File]::WriteAllBytes("$Out.validated.bin", $validated)
        Write-Output "Wrote $Out.validated.bin"

        if ($NoPrompt) { return }

        $ret = [DevModeDialog]::DocumentProperties([DevModeDialog]::GetForegroundWindow(), $hPrinter, $Queue, $buf, $buf,
            [DevModeDialog]::DM_IN_BUFFER -bor [DevModeDialog]::DM_IN_PROMPT -bor [DevModeDialog]::DM_OUT_BUFFER)
        if ($ret -ne [DevModeDialog]::IDOK) {
            Write-Output "CANCELLED (nothing written)"
            return
        }

        $captured = New-Object byte[] $size
        [System.Runtime.InteropServices.Marshal]::Copy($buf, $captured, 0, $size)
        [System.IO.File]::WriteAllBytes($Out, $captured)
        Write-Output "Wrote $Out"
    } finally {
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($buf)
    }
} finally {
    [void][DevModeDialog]::ClosePrinter($hPrinter)
}
