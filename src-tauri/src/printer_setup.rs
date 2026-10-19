//! Guided printer setup: open the driver dialog pre-filled for the preset's
//! paper size, check the captured settings for borderless expansion, and print
//! a measurement sheet.
//!
//! Expansion check: drivers that enlarge borderless prints (e.g. Epson "Auto
//! Expand") report a logical dpi higher than the DEVMODE's print quality, so the
//! app renders for e.g. 758 dpi and the printer lays it down at 720 (x1.0528).
//! See docs/spikes/printer-auto-setup.md on the spike branch.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpansionStatus {
    /// Logical dpi equals print quality: the driver isn't enlarging the page.
    NoExpansionDetected,
    /// Logical dpi is a few percent above print quality: prints are enlarged.
    Expanding,
    /// The driver doesn't report a usable print quality, or the numbers don't
    /// look like expansion; only a test print can tell.
    Unknown,
    /// A Windows class driver that sends true-size pages and leaves borderless
    /// enlargement to the printer's firmware, which the check can't see. Only a
    /// test print can tell.
    PrinterDecides,
}

/// Microsoft class drivers that hand borderless to the printer (IPP
/// `media-col` with zero margins) rather than enlarging the page themselves.
const PRINTER_SIDE_BORDERLESS_DRIVERS: &[&str] =
    &["Microsoft IPP Class Driver", "Universal Print Class Driver"];

/// Downgrades a "no expansion" result to `PrinterDecides` for class drivers,
/// whose dpi always matches because the enlarging, if any, happens in the
/// printer. An `Expanding` result stands.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn account_for_driver(status: ExpansionStatus, driver_name: Option<&str>) -> ExpansionStatus {
    let printer_side = driver_name.is_some_and(|name| {
        PRINTER_SIDE_BORDERLESS_DRIVERS
            .iter()
            .any(|d| d.eq_ignore_ascii_case(name.trim()))
    });
    if printer_side && status != ExpansionStatus::Expanding {
        ExpansionStatus::PrinterDecides
    } else {
        status
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpansionCheck {
    pub status: ExpansionStatus,
    pub logical_dpi: i32,
    pub print_quality_dpi: Option<i32>,
    /// logical ÷ print quality; e.g. 1.0528 for a 5.3% enlargement.
    pub factor: Option<f64>,
    /// Page the driver gives the app (may include bleed beyond the paper).
    pub page_mm: Option<(f64, f64)>,
    /// Paper size from the DEVMODE, when present.
    pub paper_mm: Option<(f64, f64)>,
}

/// Borderless expansion is a few percent; anything larger means the driver's
/// logical dpi simply differs from its print quality (e.g. 600 vs 300).
const MAX_EXPANSION_FACTOR: f64 = 1.15;
const SAME_DPI_TOLERANCE: f64 = 0.002;

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn classify_expansion(
    logical_dpi: i32,
    print_quality_dpi: Option<i32>,
) -> (ExpansionStatus, Option<f64>) {
    let Some(quality) = print_quality_dpi.filter(|q| *q > 0) else {
        return (ExpansionStatus::Unknown, None);
    };
    if logical_dpi <= 0 {
        return (ExpansionStatus::Unknown, None);
    }

    let factor = logical_dpi as f64 / quality as f64;
    let status = if (factor - 1.0).abs() <= SAME_DPI_TOLERANCE {
        ExpansionStatus::NoExpansionDetected
    } else if factor > 1.0 && factor <= MAX_EXPANSION_FACTOR {
        ExpansionStatus::Expanding
    } else {
        ExpansionStatus::Unknown
    };
    (status, Some(factor))
}

#[cfg(target_os = "windows")]
pub use windows_impl::{check_expansion, open_printer_dialog, print_test_sheet};

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{account_for_driver, classify_expansion, ExpansionCheck};
    use crate::models::Preset;
    use crate::parser::{find_paper_size, paper_size_mm};
    use crate::test_sheet;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    #[link(name = "user32")]
    extern "system" {
        fn EnableWindow(hwnd: isize, enable: i32) -> i32;
    }

    /// Runs a PowerShell script from a temp file and returns (stdout, stderr).
    fn run_powershell(script: &str, label: &str) -> Result<(String, String), String> {
        let script_path = std::env::temp_dir().join(format!(
            "helloprint_{}_{}.ps1",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        std::fs::write(&script_path, script)
            .map_err(|e| format!("Failed to write {} script: {}", label, e))?;

        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script_path.display().to_string(),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Failed to launch powershell: {}", e));

        std::fs::remove_file(&script_path).ok();

        let output = output?;
        Ok((
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }

    const WINSPOOL_TYPES: &str = r#"
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class PrinterDialog {
    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool OpenPrinter(string pPrinterName, out IntPtr phPrinter, IntPtr pDefault);

    [DllImport("winspool.drv", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern int DocumentProperties(
        IntPtr hWnd, IntPtr hPrinter, string pDeviceName,
        IntPtr pDevModeOutput, IntPtr pDevModeInput, int fMode);

    [DllImport("winspool.drv", SetLastError = true)]
    public static extern bool ClosePrinter(IntPtr hPrinter);

    public const int DM_IN_BUFFER  = 8;
    public const int DM_IN_PROMPT  = 4;
    public const int DM_OUT_BUFFER = 2;
    public const int IDOK = 1;
}
"@
"#;

    /// Opens the driver's own settings dialog and returns the resulting DEVMODE
    /// as base64.
    ///
    /// Starting point, in order:
    /// - `seed_devmode_base64`: the preset's saved settings, so "fix it" edits
    ///   them instead of starting over;
    /// - otherwise the printer's defaults, pre-filled through standard Print
    ///   Ticket features with the keyword's paper size, borderless and
    ///   portrait when the driver offers them.
    ///
    /// Pre-filling is best effort: on any failure the dialog opens with the
    /// printer's defaults.
    ///
    /// The dialog is owned by `owner_hwnd` (the app window, 0 for none) so it
    /// opens in front of the app rather than behind it. Blocks until the
    /// dialog closes, so never call it on the main thread.
    pub fn open_printer_dialog(
        printer_id: &str,
        paper_size_keyword: Option<&str>,
        seed_devmode_base64: Option<&str>,
        owner_hwnd: isize,
    ) -> Result<String, String> {
        let (width_um, height_um) = paper_size_keyword
            .and_then(paper_size_mm)
            .map(|(w, h)| ((w * 1000.0).round() as i64, (h * 1000.0).round() as i64))
            .unwrap_or((0, 0));

        let script = format!(
            r#"$ErrorActionPreference = 'Stop'
try {{
{winspool_types}
    $printerName = '{printer_name}'
    $seedB64 = '{seed}'
    $sizeWidthUm = {width_um}
    $sizeHeightUm = {height_um}
    $ownerHwnd = [IntPtr]::new([long]{owner_hwnd})

    $hPrinter = [IntPtr]::Zero
    if (-not [PrinterDialog]::OpenPrinter($printerName, [ref]$hPrinter, [IntPtr]::Zero)) {{
        throw "OpenPrinter failed for '$printerName' (error $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error()))"
    }}

    try {{
        $cbNeeded = [PrinterDialog]::DocumentProperties([IntPtr]::Zero, $hPrinter, $printerName, [IntPtr]::Zero, [IntPtr]::Zero, 0)
        if ($cbNeeded -le 0) {{
            throw "DocumentProperties size query failed"
        }}
        $pDevMode = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($cbNeeded)

        try {{
            function Reset-Defaults {{
                [void][PrinterDialog]::DocumentProperties([IntPtr]::Zero, $hPrinter, $printerName, $pDevMode, [IntPtr]::Zero, [PrinterDialog]::DM_OUT_BUFFER)
            }}

            # Copy a DEVMODE in and let the driver validate it.
            function Set-DevMode([byte[]]$bytes) {{
                [System.Runtime.InteropServices.Marshal]::Copy($bytes, 0, $pDevMode, [Math]::Min($bytes.Length, $cbNeeded))
                $ret = [PrinterDialog]::DocumentProperties([IntPtr]::Zero, $hPrinter, $printerName, $pDevMode, $pDevMode,
                    [PrinterDialog]::DM_IN_BUFFER -bor [PrinterDialog]::DM_OUT_BUFFER)
                if ($ret -ne [PrinterDialog]::IDOK) {{ throw "driver rejected the settings ($ret)" }}
            }}

            Reset-Defaults

            if ($seedB64) {{
                try {{
                    Set-DevMode ([System.Convert]::FromBase64String($seedB64))
                }} catch {{
                    [Console]::Error.WriteLine("PREFILL_WARNING: saved settings not accepted: $($_.Exception.Message)")
                    Reset-Defaults
                }}
            }} elseif ($sizeWidthUm -gt 0) {{
                try {{
                    Add-Type -AssemblyName System.Printing, ReachFramework
                    $psf = 'http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework'
                    $psk = 'http://schemas.microsoft.com/windows/2003/08/printing/printschemakeywords'

                    $defaults = New-Object byte[] $cbNeeded
                    [System.Runtime.InteropServices.Marshal]::Copy($pDevMode, $defaults, 0, $cbNeeded)

                    $server = New-Object System.Printing.LocalPrintServer
                    $queue = $server.GetPrintQueue($printerName)
                    $converter = New-Object System.Printing.Interop.PrintTicketConverter($printerName, $queue.ClientPrintSchemaVersion)
                    $base = $converter.ConvertDevModeToPrintTicket($defaults)

                    $caps = New-Object System.Xml.XmlDocument
                    $caps.Load($queue.GetPrintCapabilitiesAsXml($base))
                    $ns = New-Object System.Xml.XmlNamespaceManager($caps.NameTable)
                    $ns.AddNamespace('psf', $psf)

                    $hasBorderless = $null -ne $caps.SelectSingleNode("//psf:Feature[@name='psk:PageBorderless']/psf:Option[@name='psk:Borderless']", $ns)

                    # Media sizes matching the keyword's dimensions (either orientation, +-1 mm).
                    $sizes = @()
                    foreach ($opt in $caps.SelectNodes("//psf:Feature[@name='psk:PageMediaSize']/psf:Option", $ns)) {{
                        $w = $opt.SelectSingleNode("psf:ScoredProperty[@name='psk:MediaSizeWidth']/psf:Value", $ns)
                        $h = $opt.SelectSingleNode("psf:ScoredProperty[@name='psk:MediaSizeHeight']/psf:Value", $ns)
                        if (-not $w -or -not $h) {{ continue }}
                        $w = [int64]$w.InnerText; $h = [int64]$h.InnerText
                        $fits = ([Math]::Abs($w - $sizeWidthUm) -le 1000 -and [Math]::Abs($h - $sizeHeightUm) -le 1000) -or
                                ([Math]::Abs($w - $sizeHeightUm) -le 1000 -and [Math]::Abs($h - $sizeWidthUm) -le 1000)
                        if (-not $fits) {{ continue }}
                        $label = $opt.GetAttribute('name') + ' ' + ($opt.SelectSingleNode("psf:Property[@name='psk:DisplayName']/psf:Value", $ns) | ForEach-Object {{ $_.InnerText }})
                        $sizes += [pscustomobject]@{{ Name = $opt.GetAttribute('name'); Width = $w; Height = $h; Borderless = ($label -match 'borderless') }}
                    }}

                    # Drivers with a Borderless switch: pick the plain size and turn
                    # the switch on. Drivers without one often list "(Borderless)"
                    # sizes instead: prefer those.
                    $size = if ($hasBorderless) {{
                        ($sizes | Where-Object {{ -not $_.Borderless }} | Select-Object -First 1), ($sizes | Select-Object -First 1) | Where-Object {{ $_ }} | Select-Object -First 1
                    }} else {{
                        ($sizes | Where-Object {{ $_.Borderless }} | Select-Object -First 1), ($sizes | Select-Object -First 1) | Where-Object {{ $_ }} | Select-Object -First 1
                    }}

                    $features = @()
                    if ($size) {{
                        $features += "<psf:Feature name='psk:PageMediaSize'><psf:Option name='$($size.Name)'><psf:ScoredProperty name='psk:MediaSizeWidth'><psf:Value xsi:type='xsd:integer'>$($size.Width)</psf:Value></psf:ScoredProperty><psf:ScoredProperty name='psk:MediaSizeHeight'><psf:Value xsi:type='xsd:integer'>$($size.Height)</psf:Value></psf:ScoredProperty></psf:Option></psf:Feature>"
                    }}
                    if ($hasBorderless) {{
                        $features += "<psf:Feature name='psk:PageBorderless'><psf:Option name='psk:Borderless' /></psf:Feature>"
                    }}
                    $features += "<psf:Feature name='psk:PageOrientation'><psf:Option name='psk:Portrait' /></psf:Feature>"

                    # Option names may use the driver's own namespace prefixes;
                    # the size values need xsi/xsd.
                    $prefixes = @{{
                        psf = $psf
                        psk = $psk
                        xsi = 'http://www.w3.org/2001/XMLSchema-instance'
                        xsd = 'http://www.w3.org/2001/XMLSchema'
                    }}
                    foreach ($attr in $caps.DocumentElement.Attributes) {{
                        if ($attr.Prefix -eq 'xmlns' -and -not $prefixes.ContainsKey($attr.LocalName)) {{ $prefixes[$attr.LocalName] = $attr.Value }}
                    }}
                    $nsDecls = ($prefixes.GetEnumerator() | ForEach-Object {{ "xmlns:$($_.Key)='$($_.Value)'" }}) -join ' '
                    $deltaXml = "<?xml version='1.0' encoding='UTF-8'?><psf:PrintTicket $nsDecls version='1'>$($features -join '')</psf:PrintTicket>"
                    $delta = New-Object System.Printing.PrintTicket(New-Object System.IO.MemoryStream(, [System.Text.Encoding]::UTF8.GetBytes($deltaXml)))
                    $merged = $queue.MergeAndValidatePrintTicket($base, $delta).ValidatedPrintTicket
                    Set-DevMode ($converter.ConvertPrintTicketToDevMode($merged, [System.Printing.Interop.BaseDevModeType]::UserDefault))

                    $converter.Dispose()
                    $queue.Dispose()
                    $server.Dispose()
                }} catch {{
                    [Console]::Error.WriteLine("PREFILL_WARNING: $($_.Exception.Message)")
                    Reset-Defaults
                }}
            }}

            $ret = [PrinterDialog]::DocumentProperties($ownerHwnd, $hPrinter, $printerName, $pDevMode, $pDevMode,
                [PrinterDialog]::DM_IN_BUFFER -bor [PrinterDialog]::DM_IN_PROMPT -bor [PrinterDialog]::DM_OUT_BUFFER)

            if ($ret -ne [PrinterDialog]::IDOK) {{
                Write-Output "CANCELLED"
                exit 0
            }}

            $devModeBytes = New-Object byte[] $cbNeeded
            [System.Runtime.InteropServices.Marshal]::Copy($pDevMode, $devModeBytes, 0, $cbNeeded)
            Write-Output "DEVMODE:$([System.Convert]::ToBase64String($devModeBytes))"
        }} finally {{
            [System.Runtime.InteropServices.Marshal]::FreeHGlobal($pDevMode)
        }}
    }} finally {{
        [void][PrinterDialog]::ClosePrinter($hPrinter)
    }}
}} catch {{
    Write-Error "DIALOG_ERROR: $($_.Exception.Message)"
    exit 1
}}
"#,
            winspool_types = WINSPOOL_TYPES,
            printer_name = printer_id.replace('\'', "''"),
            seed = seed_devmode_base64.unwrap_or("").replace('\'', ""),
            width_um = width_um,
            height_um = height_um,
            owner_hwnd = owner_hwnd,
        );

        log::info!(
            "Opening printer dialog: printer={}, keyword={:?}, seeded={}",
            printer_id,
            paper_size_keyword,
            seed_devmode_base64.is_some()
        );
        let started = std::time::Instant::now();
        let result = run_powershell(&script, "dialog");

        // A modal driver dialog disables its owner until it closes. If the
        // PowerShell process died with the dialog open, the app window would
        // stay disabled, so always re-enable it.
        if owner_hwnd != 0 {
            unsafe {
                EnableWindow(owner_hwnd, 1);
            }
        }

        let (stdout, stderr) = result?;
        log::info!(
            "Printer dialog closed after {:.1}s",
            started.elapsed().as_secs_f64()
        );

        if !stderr.is_empty() {
            log::warn!("Printer dialog stderr: {}", stderr);
        }

        if stdout.lines().any(|l| l.trim() == "CANCELLED") {
            log::info!("Printer dialog cancelled");
            return Err("User cancelled the dialog".to_string());
        }

        if let Some(b64) = stdout
            .lines()
            .find_map(|l| l.trim().strip_prefix("DEVMODE:"))
        {
            return Ok(b64.to_string());
        }

        log::error!(
            "Printer dialog failed: stdout={}, stderr={}",
            stdout,
            stderr
        );

        Err(format!(
            "Printer dialog failed: {}",
            if !stderr.is_empty() { &stderr } else { &stdout }
        ))
    }

    /// Reads the page geometry the driver reports for these settings (no
    /// printing) and classifies borderless expansion.
    pub fn check_expansion(
        printer_id: &str,
        devmode_base64: &str,
    ) -> Result<ExpansionCheck, String> {
        let script = format!(
            r#"$ErrorActionPreference = 'Stop'
try {{
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class DeviceGeometry {{
    [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr CreateDC(string driver, string device, string output, IntPtr devMode);
    [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr hdc, int index);
    [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr hdc);

    // LOGPIXELSX, LOGPIXELSY, PHYSICALWIDTH, PHYSICALHEIGHT
    public static int[] Read(string printer, byte[] devMode) {{
        IntPtr dm = Marshal.AllocHGlobal(devMode.Length);
        Marshal.Copy(devMode, 0, dm, devMode.Length);
        IntPtr hdc = CreateDC("WINSPOOL", printer, null, dm);
        Marshal.FreeHGlobal(dm);
        if (hdc == IntPtr.Zero) throw new Exception("CreateDC failed");
        int[] idx = {{ 88, 90, 110, 111 }};
        int[] r = new int[idx.Length];
        for (int i = 0; i < idx.Length; i++) r[i] = GetDeviceCaps(hdc, idx[i]);
        DeleteDC(hdc);
        return r;
    }}
}}
"@

    $bytes = [System.Convert]::FromBase64String('{devmode}')
    $caps = [DeviceGeometry]::Read('{printer_name}', $bytes)
    # DEVMODEW public fields: dmFields, dmPaperLength/Width (0.1 mm), dmPrintQuality, dmYResolution
    $fields = [BitConverter]::ToUInt32($bytes, 72)
    $paperLength = [BitConverter]::ToInt16($bytes, 80)
    $paperWidth = [BitConverter]::ToInt16($bytes, 82)
    $quality = [BitConverter]::ToInt16($bytes, 90)
    $yResolution = [BitConverter]::ToInt16($bytes, 96)
    Write-Output ("CHECK:{{0}},{{1}},{{2}},{{3}},{{4}},{{5}},{{6}},{{7}},{{8}}" -f $caps[0], $caps[1], $caps[2], $caps[3], $quality, $yResolution, $fields, $paperWidth, $paperLength)
}} catch {{
    Write-Error "CHECK_ERROR: $($_.Exception.Message)"
    exit 1
}}
"#,
            devmode = devmode_base64.replace('\'', ""),
            printer_name = printer_id.replace('\'', "''"),
        );

        let (stdout, stderr) = run_powershell(&script, "check")?;

        let values: Vec<i64> = stdout
            .lines()
            .find_map(|l| l.trim().strip_prefix("CHECK:"))
            .ok_or_else(|| {
                format!(
                    "Expansion check failed: {}",
                    if !stderr.is_empty() { &stderr } else { &stdout }
                )
            })?
            .split(',')
            .map(|v| {
                v.trim()
                    .parse::<i64>()
                    .map_err(|e| format!("Bad check output '{}': {}", v, e))
            })
            .collect::<Result<_, _>>()?;

        let [logical_x, logical_y, physical_w, physical_h, quality, y_resolution, fields, paper_w, paper_l] =
            values[..]
        else {
            return Err(format!("Unexpected check output: {}", stdout));
        };

        // Compare like with like: print quality is the horizontal dpi; fall
        // back to the vertical resolution when quality is a negative
        // "draft/high" code.
        let (logical_dpi, print_quality_dpi) = if quality > 0 {
            (logical_x as i32, Some(quality as i32))
        } else if y_resolution > 0 {
            (logical_y as i32, Some(y_resolution as i32))
        } else {
            (logical_x as i32, None)
        };
        let (status, factor) = classify_expansion(logical_dpi, print_quality_dpi);
        let driver = printers::get_printer_by_name(printer_id).map(|p| p.driver_name);
        let status = account_for_driver(status, driver.as_deref());
        log::info!(
            "Expansion check: printer={}, driver={:?}, logical={}, quality={:?}, status={:?}",
            printer_id,
            driver,
            logical_dpi,
            print_quality_dpi,
            status
        );

        const DM_PAPERLENGTH: i64 = 0x4;
        const DM_PAPERWIDTH: i64 = 0x8;
        let paper_mm = (fields & DM_PAPERLENGTH != 0
            && fields & DM_PAPERWIDTH != 0
            && paper_w > 0
            && paper_l > 0)
            .then(|| (paper_w as f64 / 10.0, paper_l as f64 / 10.0));
        let page_mm = (logical_x > 0 && logical_y > 0).then(|| {
            (
                physical_w as f64 / logical_x as f64 * 25.4,
                physical_h as f64 / logical_y as f64 * 25.4,
            )
        });

        Ok(ExpansionCheck {
            status,
            logical_dpi,
            print_quality_dpi,
            factor,
            page_mm,
            paper_mm,
        })
    }

    /// Prints the reference test sheet (see `crate::test_sheet`) for the paper
    /// size with the given settings. Lines only, to keep ink use low.
    pub fn print_test_sheet(
        printer_id: &str,
        paper_size_keyword: &str,
        devmode_base64: Option<&str>,
    ) -> Result<(), String> {
        let size = find_paper_size(paper_size_keyword)
            .ok_or_else(|| format!("Unknown paper size '{}'", paper_size_keyword))?;

        let path = std::env::temp_dir().join(format!(
            "helloprint-test-sheet-{}-{}.png",
            size.slug,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        test_sheet::save(&path, size.width_mm, size.height_mm, size.label)?;

        let mut preset = Preset::new("Test sheet".to_string(), paper_size_keyword.to_string());
        preset.printer_id = Some(printer_id.to_string());
        preset.devmode_base64 = devmode_base64.map(str::to_string);

        let result = crate::watcher::submit_print_windows(&path, printer_id, &preset);
        std::fs::remove_file(&path).ok();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_dpi_is_no_expansion() {
        assert_eq!(
            classify_expansion(720, Some(720)).0,
            ExpansionStatus::NoExpansionDetected
        );
    }

    #[test]
    fn epson_auto_expand_is_expanding() {
        let (status, factor) = classify_expansion(758, Some(720));
        assert_eq!(status, ExpansionStatus::Expanding);
        assert!((factor.unwrap() - 1.0528).abs() < 0.0001);
    }

    #[test]
    fn unrelated_dpi_difference_is_unknown() {
        assert_eq!(
            classify_expansion(600, Some(300)).0,
            ExpansionStatus::Unknown
        );
        assert_eq!(
            classify_expansion(300, Some(600)).0,
            ExpansionStatus::Unknown
        );
    }

    #[test]
    fn missing_quality_is_unknown() {
        assert_eq!(classify_expansion(600, None).0, ExpansionStatus::Unknown);
        assert_eq!(
            classify_expansion(600, Some(-4)).0,
            ExpansionStatus::Unknown
        );
    }

    #[test]
    fn ipp_class_driver_leaves_it_to_the_printer() {
        assert_eq!(
            account_for_driver(
                ExpansionStatus::NoExpansionDetected,
                Some("Microsoft IPP Class Driver")
            ),
            ExpansionStatus::PrinterDecides
        );
        assert_eq!(
            account_for_driver(
                ExpansionStatus::Unknown,
                Some("Universal Print Class Driver")
            ),
            ExpansionStatus::PrinterDecides
        );
    }

    #[test]
    fn vendor_drivers_and_measured_expansion_are_unchanged() {
        assert_eq!(
            account_for_driver(
                ExpansionStatus::NoExpansionDetected,
                Some("EPSON ET-8500 Series")
            ),
            ExpansionStatus::NoExpansionDetected
        );
        assert_eq!(
            account_for_driver(
                ExpansionStatus::Expanding,
                Some("Microsoft IPP Class Driver")
            ),
            ExpansionStatus::Expanding
        );
        assert_eq!(
            account_for_driver(ExpansionStatus::NoExpansionDetected, None),
            ExpansionStatus::NoExpansionDetected
        );
    }
}
