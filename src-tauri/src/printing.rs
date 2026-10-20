use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub is_default: bool,
    pub is_online: bool,
    /// On Windows' generic driver instead of the manufacturer's; see
    /// `is_generic_driver`.
    #[serde(default)]
    pub is_generic_driver: bool,
}

/// Windows' generic drivers, which Windows 11 prefers for newly added printers
/// since July 2026. They have no setting to stop a printer enlarging
/// borderless prints (Epson Retain Size and the like live in the
/// manufacturer's driver), so true-size borderless needs that driver.
const GENERIC_DRIVERS: &[&str] = &["Microsoft IPP Class Driver", "Universal Print Class Driver"];

pub fn is_generic_driver(driver_name: &str) -> bool {
    GENERIC_DRIVERS
        .iter()
        .any(|d| d.eq_ignore_ascii_case(driver_name.trim()))
}

/// A single printer option (e.g. "PageSize", "MediaType", "InputSlot").
/// Each has a list of choices and an optional default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterOption {
    /// Internal key (e.g. "PageSize", "MediaType", "InputSlot")
    pub key: String,
    /// Human-readable label (e.g. "Media Size", "Media Type", "Media Source")
    pub label: String,
    /// Available choices, in order
    pub choices: Vec<PrinterOptionChoice>,
    /// The key of the default choice, if any
    pub default_choice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterOptionChoice {
    /// Internal value (e.g. "4x6.Fullbleed", "photographic-glossy")
    pub value: String,
    /// Display label — same as value unless we can derive a better one
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterCapabilities {
    /// All options reported by the driver, keyed by option key
    pub options: Vec<PrinterOption>,
}

pub fn discover_printers() -> Vec<PrinterInfo> {
    // On macOS/Linux, use lpstat to get the real CUPS queue names.
    // The `printers` crate returns display names (with spaces/hyphens) that
    // don't work with lpoptions/lp commands which need the queue name
    // (underscored form).
    #[cfg(not(target_os = "windows"))]
    {
        discover_printers_cups()
    }
    #[cfg(target_os = "windows")]
    {
        discover_printers_windows()
    }
}

/// The Windows default printer's name. Not `printers::get_default_printer()`:
/// printers 2.3.0 compares only the first character of the names, so any
/// printer sharing the default's first letter could be reported as default.
#[cfg(target_os = "windows")]
fn default_printer_name_windows() -> Option<String> {
    #[link(name = "winspool")]
    extern "system" {
        fn GetDefaultPrinterW(buffer: *mut u16, size: *mut u32) -> i32;
    }

    let mut size: u32 = 0;
    unsafe {
        GetDefaultPrinterW(std::ptr::null_mut(), &mut size);
    }
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0u16; size as usize];
    if unsafe { GetDefaultPrinterW(buffer.as_mut_ptr(), &mut size) } == 0 {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}

#[cfg(target_os = "windows")]
fn discover_printers_windows() -> Vec<PrinterInfo> {
    let system_printers = printers::get_printers();
    let default_name = default_printer_name_windows();

    system_printers
        .into_iter()
        .map(|p| {
            let is_default = default_name.as_deref() == Some(&p.name);
            PrinterInfo {
                id: p.name.clone(),
                name: p.name.clone(),
                is_generic_driver: is_generic_driver(&p.driver_name),
                driver: p.driver_name,
                is_default,
                is_online: true,
            }
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn discover_printers_cups() -> Vec<PrinterInfo> {
    use std::process::Command;

    let mut printers_list = Vec::new();

    // Get default printer
    let default_name = Command::new("lpstat")
        .args(["-d"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                // Output: "system default destination: PRINTER_NAME"
                text.split(':').nth(1).map(|s| s.trim().to_string())
            } else {
                None
            }
        });

    // Get all printers via lpstat -p
    let output = Command::new("lpstat").args(["-p"]).output();

    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                // Lines look like: "printer QUEUE_NAME is idle.  enabled since ..."
                //               or: "printer QUEUE_NAME disabled since ..."
                if let Some(rest) = line.strip_prefix("printer ") {
                    // Queue name is the next token
                    if let Some(queue_name) = rest.split_whitespace().next() {
                        let is_default = default_name.as_deref() == Some(queue_name);
                        let is_online = !rest.contains("disabled");
                        // Use queue name as both id and display name — it's what CUPS commands need
                        printers_list.push(PrinterInfo {
                            id: queue_name.to_string(),
                            name: queue_name.replace('_', " "),
                            driver: String::new(),
                            is_default,
                            is_online,
                            is_generic_driver: false,
                        });
                    }
                }
            }
        }
    }

    // Fallback to printers crate if lpstat returned nothing
    if printers_list.is_empty() {
        let system_printers = printers::get_printers();
        let default_printer = printers::get_default_printer();
        let default_name = default_printer.as_ref().map(|p| p.name.clone());

        printers_list = system_printers
            .into_iter()
            .map(|p| {
                let is_default = default_name.as_deref() == Some(&p.name);
                PrinterInfo {
                    id: p.name.clone(),
                    name: p.name.clone(),
                    is_generic_driver: is_generic_driver(&p.driver_name),
                    driver: p.driver_name,
                    is_default,
                    is_online: true,
                }
            })
            .collect();
    }

    printers_list
}

// ─── macOS / Linux: parse `lpoptions -p <printer> -l` ───────────────────────

#[cfg(not(target_os = "windows"))]
pub fn get_printer_capabilities(printer_id: &str) -> PrinterCapabilities {
    use std::process::Command;

    let output = Command::new("lpoptions")
        .args(["-p", printer_id, "-l"])
        .output();

    let options = match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            parse_lpoptions(&text)
        }
        _ => vec![],
    };

    let mut options = options;

    // Add CUPS print-scaling option (controls image scaling before driver sees it)
    if !options.iter().any(|o| o.key == "print-scaling") {
        options.push(PrinterOption {
            key: "print-scaling".to_string(),
            label: "Image Scaling".to_string(),
            choices: vec![
                PrinterOptionChoice {
                    value: "none".to_string(),
                    label: "None (original size)".to_string(),
                },
                PrinterOptionChoice {
                    value: "fit".to_string(),
                    label: "Fit to page".to_string(),
                },
                PrinterOptionChoice {
                    value: "fill".to_string(),
                    label: "Fill page (may crop)".to_string(),
                },
                PrinterOptionChoice {
                    value: "auto".to_string(),
                    label: "Auto (CUPS default)".to_string(),
                },
            ],
            default_choice: Some("none".to_string()),
        });
    }

    // Always append an orientation option (not always in lpoptions output)
    let has_orientation = options
        .iter()
        .any(|o| o.key.to_lowercase().contains("orientation"));
    if !has_orientation {
        options.push(PrinterOption {
            key: "orientation-requested".to_string(),
            label: "Orientation".to_string(),
            choices: vec![
                PrinterOptionChoice {
                    value: "3".to_string(),
                    label: "Portrait".to_string(),
                },
                PrinterOptionChoice {
                    value: "4".to_string(),
                    label: "Landscape".to_string(),
                },
                PrinterOptionChoice {
                    value: "5".to_string(),
                    label: "Reverse Landscape".to_string(),
                },
                PrinterOptionChoice {
                    value: "6".to_string(),
                    label: "Reverse Portrait".to_string(),
                },
            ],
            default_choice: Some("3".to_string()),
        });
    }

    PrinterCapabilities { options }
}

/// Vendor options that are useful and should be kept (with label mappings).
/// All other vendor options (EPIJ_, CNIj, BR*) are filtered out.
#[cfg(not(target_os = "windows"))]
const VENDOR_ALLOWLIST: &[&str] = &[
    "EPIJ_Qual",     // Print Quality
    "EPIJ_exmg",     // Borderless Expansion
    "EPIJ_DSPr",     // 2-sided Printing
    "EPIJ_Silt",     // Quiet Mode
    "EPIJ_OSColMat", // Color Matching (Vendor vs ColorSync)
    "EPIJ_CMat",     // Color Settings (Manual/Fix Photo/Off)
];

/// Returns true for vendor-specific option keys that should be filtered out.
/// Allowlisted vendor options are kept.
#[cfg(not(target_os = "windows"))]
fn is_vendor_option(key: &str) -> bool {
    let is_vendor = key.starts_with("EPIJ_")
        || key.starts_with("EPIJ")
        || key.starts_with("CNIj")
        || key.starts_with("BR");

    if !is_vendor {
        return false;
    }

    // Keep allowlisted vendor options
    if VENDOR_ALLOWLIST
        .iter()
        .any(|allowed| key.starts_with(allowed))
    {
        return false;
    }

    true
}

/// Map Epson print quality numeric codes to human-readable names.
/// Source: EPSON_ET_8500_Series.ppd
#[cfg(not(target_os = "windows"))]
fn epson_quality_label(code: &str) -> Option<&'static str> {
    match code {
        "302" => Some("Economy"),
        "303" => Some("Normal"),
        "304" => Some("Fine"),
        "305" => Some("Quality"),
        "306" => Some("High Quality"),
        "307" => Some("Best Quality"),
        "308" => Some("Draft"),
        _ => None,
    }
}

/// Map Epson borderless expansion values to human-readable names.
/// Source: EPSON_ET_8500_Series.ppd — only 3 levels, all involve some expansion.
/// Borderless always scales slightly to bleed past edges.
#[cfg(not(target_os = "windows"))]
fn epson_expansion_label(code: &str) -> Option<&'static str> {
    match code {
        "0" => Some("Min"),
        "1" => Some("Mid"),
        "2" => Some("Standard"),
        _ => None,
    }
}

/// Map common Epson on/off/mode values.
#[cfg(not(target_os = "windows"))]
fn epson_on_off_label(code: &str) -> Option<&'static str> {
    match code {
        "0" => Some("Off"),
        "1" => Some("On"),
        "2" => Some("On (Low Noise)"),
        _ => None,
    }
}

/// Map Epson ColorMatching values.
#[cfg(not(target_os = "windows"))]
fn epson_color_matching_label(code: &str) -> Option<&'static str> {
    match code {
        "1" => Some("Epson Vendor Matching"),
        "2" => Some("ColorSync (macOS)"),
        _ => None,
    }
}

/// Map Epson Color Settings values.
#[cfg(not(target_os = "windows"))]
fn epson_color_settings_label(code: &str) -> Option<&'static str> {
    match code {
        "0" => Some("Manual Settings"),
        "1" => Some("Fix Photo"),
        "3" => Some("Off (No Color Adjustment)"),
        _ => None,
    }
}

/// Detect options that are slider-style ranges (many consecutive integers).
/// These don't work well as dropdowns.
#[cfg(not(target_os = "windows"))]
fn is_numeric_range(choices: &[PrinterOptionChoice]) -> bool {
    if choices.len() < 10 {
        return false;
    }
    choices.iter().all(|c| c.value.parse::<i64>().is_ok())
}

/// Map Epson media type numeric codes to human-readable names.
#[cfg(not(target_os = "windows"))]
fn epson_media_type_label(code: &str) -> Option<&'static str> {
    match code {
        "0" => Some("Plain Paper"),
        "2" => Some("Matte Paper"),
        "12" => Some("Bright White Paper"),
        "13" => Some("Photo Quality Ink Jet"),
        "15" => Some("Premium Glossy Photo Paper"),
        "26" => Some("Premium Semi-Gloss Photo Paper"),
        "27" => Some("Ultra Premium Glossy Photo Paper"),
        "53" => Some("Photo Paper Glossy"),
        "92" => Some("Premium Photo Paper Glossy"),
        "93" => Some("Premium Photo Paper Semi-Gloss"),
        "142" => Some("Epson Premium Photo Paper Glossy"),
        "145" => Some("Photo Quality Self-Adhesive"),
        "159" => Some("Ultra Premium Photo Paper Luster"),
        "160" => Some("Velvet Fine Art Paper"),
        "187" => Some("Premium Presentation Paper Matte"),
        _ => None,
    }
}

/// Humanize Epson PPD page size values into readable labels.
/// Handles patterns like "EP8x10in" → "8x10", "EPKG.NMgn" → "KG / 4x6 (Borderless)",
/// ".NMgn" suffix → "(Borderless)", ".ManuFeedBM20mm" → "(Manual Feed)".
#[cfg(not(target_os = "windows"))]
fn humanize_page_size(value: &str) -> String {
    // Detect suffixes first
    let (base, suffix) = if let Some(b) = value.strip_suffix(".NMgn") {
        (b, " (Borderless)")
    } else if value.contains(".ManuFeedBM") {
        // e.g. "A4.ManuFeedBM20mm" or "Legal.ManuFeedBM20mm"
        let b = value.split('.').next().unwrap_or(value);
        (b, " (Manual Feed)")
    } else if let Some(b) = value.strip_suffix(".ManuCDR") {
        (b, " (CD/DVD Tray)")
    } else if let Some(b) = value.strip_suffix(".Fullbleed") {
        (b, " (Borderless)")
    } else {
        (value, "")
    };

    // Map the base name to something readable
    let readable = match base {
        "Letter" => "Letter (8.5x11)".to_string(),
        "Legal" => "Legal (8.5x14)".to_string(),
        "Executive" => "Executive".to_string(),
        "Env10" => "#10 Envelope".to_string(),
        "A4" => "A4".to_string(),
        "A6" => "A6".to_string(),
        "EPKG" => "KG / 4x6".to_string(),
        "EP8x10in" => "8x10".to_string(),
        "EPPhotoPaper2L" => "2L / 5x7".to_string(),
        "EPPhotoPaperLRoll" => "L Roll (3.5x5)".to_string(),
        "EPHiVision102x180" => "Hi-Vision (4x7)".to_string(),
        "EPHalfLetter" => "Half Letter (5.5x8.5)".to_string(),
        "EPOficio9" => "Oficio".to_string(),
        "EP216x330mm" => "216x330mm".to_string(),
        "FolioSP" => "Folio".to_string(),
        "Statement" => "Statement (5.5x8.5)".to_string(),
        "FanFoldGermanLegal" => "Fan Fold German Legal".to_string(),
        b if b.starts_with("Custom.") => return "Custom Size".to_string(),
        // Generic: strip "EP" prefix and split
        b if b.starts_with("EP") => {
            let stripped = &b[2..];
            // Handle dimension patterns like "8x10in", "216x330mm"
            stripped.to_string()
        }
        // Standard sizes like "4x6", "5x7", "3.5x5", "8x10", "101.6x180.6mm"
        b => b.to_string(),
    };

    format!("{}{}", readable, suffix)
}

/// Parse the output of `lpoptions -p <printer> -l`.
/// Each line looks like:
///   KeyName/Display Label: choice1 *defaultChoice choice3
///
/// Filters out vendor-specific options that duplicate standard CUPS options,
/// and skips large numeric ranges that should be sliders not dropdowns.
#[cfg(not(target_os = "windows"))]
fn parse_lpoptions(text: &str) -> Vec<PrinterOption> {
    let mut options = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Split on ": " to get key/label part and choices part
        let Some((key_part, choices_part)) = line.split_once(": ") else {
            continue;
        };

        // key_part is "KeyName/Display Label" or just "KeyName"
        let (key, label) = if let Some((k, l)) = key_part.split_once('/') {
            (k.trim().to_string(), l.trim().to_string())
        } else {
            let k = key_part.trim().to_string();
            let l = humanize_option_key(&k);
            (k, l)
        };

        // Skip all vendor-specific options — they use opaque numeric codes.
        // Standard CUPS options (PageSize, MediaType, ColorModel, Resolution, etc.)
        // appear separately in the output with human-readable values.
        if is_vendor_option(&key) {
            continue;
        }

        let is_media_type = key == "MediaType";
        let is_page_size = key == "PageSize";
        let is_quality = key.starts_with("EPIJ_Qual");
        let is_expansion = key.starts_with("EPIJ_exmg");
        let is_vendor_bool = key.starts_with("EPIJ_DSPr") || key.starts_with("EPIJ_Silt");
        let is_color_matching = key.starts_with("EPIJ_OSColMat");
        let is_color_settings = key.starts_with("EPIJ_CMat");
        let has_label_mapping = is_media_type
            || is_quality
            || is_expansion
            || is_vendor_bool
            || is_color_matching
            || is_color_settings;

        let mut choices = Vec::new();
        let mut default_choice = None;

        for token in choices_part.split_whitespace() {
            let (value, is_default) = if let Some(stripped) = token.strip_prefix('*') {
                (stripped, true)
            } else {
                (token, false)
            };

            let display_label = if is_media_type {
                epson_media_type_label(value)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| humanize_choice_value(value))
            } else if is_page_size {
                humanize_page_size(value)
            } else if is_quality {
                epson_quality_label(value)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| humanize_choice_value(value))
            } else if is_expansion {
                epson_expansion_label(value)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| humanize_choice_value(value))
            } else if is_vendor_bool {
                epson_on_off_label(value)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| humanize_choice_value(value))
            } else if is_color_matching {
                epson_color_matching_label(value)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| humanize_choice_value(value))
            } else if is_color_settings {
                epson_color_settings_label(value)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| humanize_choice_value(value))
            } else {
                humanize_choice_value(value)
            };

            if is_default {
                default_choice = Some(value.to_string());
            }

            choices.push(PrinterOptionChoice {
                value: value.to_string(),
                label: display_label,
            });
        }

        // Skip options with only one choice (not useful to configure)
        if choices.len() <= 1 && key != "PageSize" {
            continue;
        }

        // Skip slider-style numeric ranges (Brightness -25..25, etc.)
        // But exempt options that have label mappings (MediaType, Quality, etc.)
        if !has_label_mapping && is_numeric_range(&choices) {
            continue;
        }

        options.push(PrinterOption {
            key,
            label,
            choices,
            default_choice,
        });
    }

    options
}

// ─── Windows: query via Print Ticket XML / PowerShell ────────────────────────

#[cfg(target_os = "windows")]
pub fn get_printer_capabilities(printer_id: &str) -> PrinterCapabilities {
    use std::process::Command;

    // Use PowerShell to get the PrintCapabilities XML and parse it
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
try {{
    Add-Type -AssemblyName System.Printing
    $server = New-Object System.Printing.LocalPrintServer
    $queue = $server.GetPrintQueue('{printer_id}')
    $stream = $queue.GetPrintCapabilitiesAsXml()
    $reader = New-Object System.IO.StreamReader($stream)
    $xml = [xml]$reader.ReadToEnd()
    $reader.Close()
    $stream.Close()

    $ns = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
    $ns.AddNamespace('psf', 'http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework')
    $ns.AddNamespace('psk', 'http://schemas.microsoft.com/windows/2003/08/printing/printschemakeywords')
    $ns.AddNamespace('xsi', 'http://www.w3.org/2001/XMLSchema-instance')

    $features = $xml.SelectNodes('//psf:Feature', $ns)
    foreach ($feature in $features) {{
        $featureName = $feature.GetAttribute('name')
        if (-not $featureName) {{ continue }}

        # Emit feature header
        Write-Output "FEATURE:$featureName"

        $options = $feature.SelectNodes('psf:Option', $ns)
        $defaultOpt = $null
        foreach ($opt in $options) {{
            $optName = $opt.GetAttribute('name')
            if (-not $optName) {{
                # Try to get the constrained value
                $val = $opt.SelectSingleNode('.//psf:Value', $ns)
                if ($val) {{ $optName = $val.InnerText }}
            }}
            if ($optName) {{
                # Check if this is marked as default
                $constrained = $opt.GetAttribute('constrained')
                Write-Output "  OPTION:$optName"
            }}
        }}
    }}
}} catch {{
    # Fallback: use Get-PrinterProperty
    try {{
        $props = Get-PrinterProperty -PrinterName '{printer_id}' -ErrorAction Stop
        foreach ($prop in $props) {{
            Write-Output "PROP:$($prop.PropertyName)=$($prop.Value)"
        }}
    }} catch {{
        Write-Output "ERROR:$($_.Exception.Message)"
    }}
}}
"#
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();

    let mut options = match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            parse_windows_capabilities(&text)
        }
        Err(_) => vec![],
    };

    // Always include orientation
    let has_orientation = options
        .iter()
        .any(|o| o.key.to_lowercase().contains("orientation"));
    if !has_orientation {
        options.push(PrinterOption {
            key: "psk:PageOrientation".to_string(),
            label: "Orientation".to_string(),
            choices: vec![
                PrinterOptionChoice {
                    value: "psk:Portrait".to_string(),
                    label: "Portrait".to_string(),
                },
                PrinterOptionChoice {
                    value: "psk:Landscape".to_string(),
                    label: "Landscape".to_string(),
                },
            ],
            default_choice: Some("psk:Portrait".to_string()),
        });
    }

    PrinterCapabilities { options }
}

#[cfg(target_os = "windows")]
fn parse_windows_capabilities(text: &str) -> Vec<PrinterOption> {
    let mut options: Vec<PrinterOption> = Vec::new();
    let mut current_feature: Option<PrinterOption> = None;
    let mut prop_map: HashMap<String, Vec<String>> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();

        if let Some(feature_name) = line.strip_prefix("FEATURE:") {
            // Save previous feature
            if let Some(feat) = current_feature.take() {
                if !feat.choices.is_empty() {
                    options.push(feat);
                }
            }

            let label = humanize_psk_key(feature_name);
            current_feature = Some(PrinterOption {
                key: feature_name.to_string(),
                label,
                choices: Vec::new(),
                default_choice: None,
            });
        } else if let Some(opt_name) = line.strip_prefix("OPTION:") {
            if let Some(ref mut feat) = current_feature {
                let label = humanize_psk_key(opt_name);
                feat.choices.push(PrinterOptionChoice {
                    value: opt_name.to_string(),
                    label,
                });
            }
        } else if let Some(prop_str) = line.strip_prefix("PROP:") {
            // Fallback: Get-PrinterProperty format "Name=Value"
            if let Some((name, value)) = prop_str.split_once('=') {
                prop_map
                    .entry(name.to_string())
                    .or_default()
                    .push(value.to_string());
            }
        }
    }

    // Save last feature
    if let Some(feat) = current_feature.take() {
        if !feat.choices.is_empty() {
            options.push(feat);
        }
    }

    // If we got properties instead of features, convert them
    if options.is_empty() && !prop_map.is_empty() {
        for (name, values) in prop_map {
            let label = humanize_option_key(&name);
            let choices: Vec<PrinterOptionChoice> = values
                .iter()
                .map(|v| PrinterOptionChoice {
                    value: v.clone(),
                    label: v.clone(),
                })
                .collect();
            if !choices.is_empty() {
                options.push(PrinterOption {
                    key: name,
                    label,
                    choices,
                    default_choice: None,
                });
            }
        }
    }

    options
}

/// Strip "psk:" prefix and split PascalCase into words.
fn humanize_psk_key(key: &str) -> String {
    let key = key.strip_prefix("psk:").unwrap_or(key);
    let key = key.strip_prefix("ns0000:").unwrap_or(key);
    split_pascal_case(key)
}

#[allow(dead_code)]
fn humanize_option_key(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "pagesize" => "Paper Size".to_string(),
        "mediatype" => "Media Type".to_string(),
        "inputslot" => "Input Slot".to_string(),
        "colormodel" => "Color Mode".to_string(),
        "cupsprintquality" => "Print Quality".to_string(),
        "duplex" => "Duplex / 2-Sided".to_string(),
        "outputbin" => "Output Bin".to_string(),
        "resolution" => "Resolution".to_string(),
        _ => split_pascal_case(key),
    }
}

#[allow(dead_code)]
fn humanize_choice_value(value: &str) -> String {
    // Strip common prefixes
    let v = value.strip_prefix("psk:").unwrap_or(value);
    let v = v.strip_prefix("ns0000:").unwrap_or(v);

    // Some known CUPS value mappings
    match v {
        "None" => "Off".to_string(),
        "DuplexNoTumble" => "Long Edge".to_string(),
        "DuplexTumble" => "Short Edge".to_string(),
        "any" => "Any / Auto".to_string(),
        "auto" => "Auto".to_string(),
        "main" => "Main Tray".to_string(),
        "rear" => "Rear Feed".to_string(),
        "photo" => "Photo Tray".to_string(),
        "manual" => "Manual Feed".to_string(),
        "disc" => "Disc Tray".to_string(),
        "stationery" => "Plain Paper".to_string(),
        "stationery-coated" => "Coated Paper".to_string(),
        "stationery-letterhead" => "Letterhead".to_string(),
        "stationery-lightweight" => "Lightweight Paper".to_string(),
        "photographic" => "Photo Paper".to_string(),
        "photographic-glossy" => "Photo Paper Glossy".to_string(),
        "photographic-high-gloss" => "Photo Paper High Gloss".to_string(),
        "photographic-semi-gloss" => "Photo Paper Semi-Gloss".to_string(),
        "photographic-matte" => "Photo Paper Matte".to_string(),
        "envelope" => "Envelope".to_string(),
        "Normal" => "Normal".to_string(),
        "High" => "High".to_string(),
        "Draft" => "Draft".to_string(),
        "Gray" => "Grayscale".to_string(),
        "RGB" => "Color (RGB)".to_string(),
        "CMYK" => "Color (CMYK)".to_string(),
        _ => {
            // If it contains a vendor prefix like "com.epson-", clean it up
            if let Some(stripped) = v.strip_prefix("com.epson-") {
                return split_kebab_case(stripped);
            }
            if v.contains('.') && v.contains("Fullbleed") {
                return format!("{} (Borderless)", v.replace(".Fullbleed", ""));
            }
            v.to_string()
        }
    }
}

fn split_pascal_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && ch.is_uppercase() {
            let prev = s.chars().nth(i - 1);
            if prev.map(|c| c.is_lowercase()).unwrap_or(false) {
                result.push(' ');
            }
        }
        result.push(ch);
    }
    result
}

#[allow(dead_code)]
fn split_kebab_case(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let mut out = first.to_uppercase().to_string();
                    out.extend(chars);
                    out
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    /// The default printer's name must match one printer exactly (no stray
    /// nulls), and only that printer is flagged as default.
    #[test]
    fn exactly_one_printer_is_default() {
        let Some(default_name) = default_printer_name_windows() else {
            return; // no default printer on this machine
        };
        let printers = discover_printers_windows();
        let defaults: Vec<_> = printers.iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1, "{printers:?}");
        assert_eq!(defaults[0].name, default_name);
    }
}

#[cfg(test)]
mod tests {
    use super::is_generic_driver;

    #[test]
    fn generic_drivers_are_recognised() {
        assert!(is_generic_driver("Microsoft IPP Class Driver"));
        assert!(is_generic_driver(" universal print class driver "));
        assert!(!is_generic_driver("EPSON ET-8500 Series"));
        assert!(!is_generic_driver(""));
    }
}
