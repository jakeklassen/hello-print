use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::event::CreateKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use crate::jobs::{JobQueueState, JobStatus, PrintJob};
#[cfg(target_os = "macos")]
use crate::macos_printing;
use crate::models::{PostFileAction, Preset};
use crate::parser::parse_size_keyword;
use crate::storage::StorageState;

/// How long after processing a file before we'll process it again.
/// This prevents duplicate events from the file watcher (Create + multiple Modify events)
/// from creating multiple queue entries.
const DEDUP_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WatcherStatus {
    Idle,
    Active,
    Paused,
    Error,
}

pub struct WatcherState {
    pub status: Mutex<WatcherStatus>,
    pub watcher: Mutex<Option<RecommendedWatcher>>,
    processed_zips: Mutex<HashSet<String>>,
    /// Tracks recently processed files with their processing timestamp.
    /// Events for the same path within DEDUP_WINDOW are ignored.
    recently_processed: Mutex<HashMap<PathBuf, Instant>>,
}

impl WatcherState {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(WatcherStatus::Idle),
            watcher: Mutex::new(None),
            processed_zips: Mutex::new(HashSet::new()),
            recently_processed: Mutex::new(HashMap::new()),
        }
    }
}

const SUPPORTED_IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "tiff", "tif"];

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_zip_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase() == "zip")
        .unwrap_or(false)
}

fn file_hash(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let hash = Sha256::digest(&data);
    Ok(hex::encode(hash))
}

fn extract_zip(zip_path: &Path, target_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut extracted = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }

        let name = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|f| f.to_os_string()))
            .ok_or_else(|| "Invalid zip entry name".to_string())?;

        let out_path = target_dir.join(&name);

        // Only extract image files
        if !is_image_file(&out_path) {
            continue;
        }

        // Avoid overwriting existing files
        if out_path.exists() {
            continue;
        }

        let mut out_file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
        extracted.push(out_path);
    }

    Ok(extracted)
}

#[derive(Clone, Serialize)]
struct FileEvent {
    event_type: String,
    path: String,
    keyword: Option<String>,
    preset_name: Option<String>,
}

pub fn start_watcher(
    app_handle: AppHandle,
    watch_folder: String,
    watcher_state: Arc<WatcherState>,
    storage_state: Arc<StorageState>,
    job_queue: Arc<JobQueueState>,
) -> Result<(), String> {
    let folder = PathBuf::from(&watch_folder);
    if !folder.exists() {
        return Err(format!("Watch folder does not exist: {}", watch_folder));
    }

    // Stop any existing watcher
    stop_watcher_inner(&watcher_state);

    let app = app_handle.clone();
    let ws = watcher_state.clone();
    let ss = storage_state.clone();
    let jq = job_queue.clone();

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        match res {
            Ok(event) => {
                if matches!(
                    event.kind,
                    EventKind::Create(CreateKind::File) | EventKind::Modify(_)
                ) {
                    for path in event.paths {
                        // Debounce: wait for file to finish writing
                        std::thread::sleep(Duration::from_millis(1500));
                        handle_file(&app, &path, &ws, &ss, &jq);
                    }
                }
            }
            Err(e) => {
                eprintln!("Watcher error: {}", e);
                if let Ok(mut status) = ws.status.lock() {
                    *status = WatcherStatus::Error;
                }
                let _ = app.emit("watcher-status", WatcherStatus::Error);
            }
        }
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(folder.as_ref(), RecursiveMode::NonRecursive)
        .map_err(|e| e.to_string())?;

    // Store the watcher and update status
    *watcher_state.watcher.lock().map_err(|e| e.to_string())? = Some(watcher);
    *watcher_state.status.lock().map_err(|e| e.to_string())? = WatcherStatus::Active;
    let _ = app_handle.emit("watcher-status", WatcherStatus::Active);

    Ok(())
}

fn handle_file(
    app: &AppHandle,
    path: &Path,
    watcher_state: &WatcherState,
    storage_state: &StorageState,
    job_queue: &JobQueueState,
) {
    if !path.exists() || !path.is_file() {
        return;
    }

    // Skip files in "printed" subfolder
    if path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n == "printed")
        .unwrap_or(false)
    {
        return;
    }

    // Skip files in "processed_zips" subfolder
    if path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n == "processed_zips")
        .unwrap_or(false)
    {
        return;
    }

    // Dedup: skip if this file was processed recently
    {
        let mut recent = watcher_state.recently_processed.lock().unwrap();
        let now = Instant::now();

        // Clean up stale entries while we have the lock
        recent.retain(|_, timestamp| now.duration_since(*timestamp) < DEDUP_WINDOW);

        if let Some(last_processed) = recent.get(path) {
            if now.duration_since(*last_processed) < DEDUP_WINDOW {
                return; // Already processed recently, skip
            }
        }

        // Mark as processing now (before the sleep) to block concurrent events
        recent.insert(path.to_path_buf(), now);
    }

    let result = if is_zip_file(path) {
        handle_zip(app, path, watcher_state, storage_state)
    } else if is_image_file(path) {
        handle_image(app, path, storage_state, job_queue)
    } else {
        Ok(())
    };

    if let Err(e) = result {
        eprintln!("Error processing {}: {}", path.display(), e);
        let _ = app.emit(
            "file-event",
            FileEvent {
                event_type: "error".to_string(),
                path: path.display().to_string(),
                keyword: None,
                preset_name: None,
            },
        );
    }
}

fn handle_zip(
    app: &AppHandle,
    zip_path: &Path,
    watcher_state: &WatcherState,
    storage_state: &StorageState,
) -> Result<(), String> {
    // Check if already processed
    let hash = file_hash(zip_path)?;
    let key = format!(
        "{}:{}",
        zip_path.file_name().unwrap_or_default().to_string_lossy(),
        hash
    );

    {
        let mut processed = watcher_state
            .processed_zips
            .lock()
            .map_err(|e| e.to_string())?;
        if processed.contains(&key) {
            return Ok(());
        }
        processed.insert(key);
    }

    let target_dir = zip_path.parent().unwrap_or(Path::new("."));
    let extracted = extract_zip(zip_path, target_dir)?;

    let _ = app.emit(
        "file-event",
        FileEvent {
            event_type: "zip_extracted".to_string(),
            path: zip_path.display().to_string(),
            keyword: None,
            preset_name: Some(format!("{} files extracted", extracted.len())),
        },
    );

    // Post-zip action
    let config = storage_state.config.lock().map_err(|e| e.to_string())?;
    match config.post_zip_action {
        PostFileAction::Delete => {
            fs::remove_file(zip_path).ok();
        }
        PostFileAction::MoveToSubfolder => {
            let processed_dir = target_dir.join("processed_zips");
            fs::create_dir_all(&processed_dir).ok();
            if let Some(name) = zip_path.file_name() {
                fs::rename(zip_path, processed_dir.join(name)).ok();
            }
        }
        PostFileAction::Keep => {}
    }

    Ok(())
}

fn handle_image(
    app: &AppHandle,
    image_path: &Path,
    storage_state: &StorageState,
    job_queue: &JobQueueState,
) -> Result<(), String> {
    let filename = image_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let keyword = parse_size_keyword(&filename);

    let presets = storage_state.presets.lock().map_err(|e| e.to_string())?;
    let config = storage_state.config.lock().map_err(|e| e.to_string())?;

    let matched_preset = if let Some(ref kw) = keyword {
        presets
            .iter()
            .find(|p| p.paper_size_keyword.to_lowercase() == kw.to_lowercase())
    } else {
        // Use default preset
        config
            .default_preset_id
            .and_then(|id| presets.iter().find(|p| p.id == id))
    };

    if let Some(preset) = matched_preset {
        // Create a job in the queue
        let mut job = PrintJob::new(
            filename.clone(),
            image_path.display().to_string(),
            Some(preset.id),
            Some(preset.name.clone()),
        );

        if preset.auto_print {
            // Mark as printing
            job.status = JobStatus::Printing;
            let job_id = job.id;
            job_queue.add_job(job);
            let _ = app.emit("job-updated", "");

            // Submit print job
            let print_result = submit_print_job(app, image_path, preset);
            match print_result {
                Ok(()) => {
                    job_queue.update_status(job_id, JobStatus::Complete, None);
                    let _ = app.emit("job-updated", "");

                    // Post-print action
                    match config.post_print_action {
                        PostFileAction::MoveToSubfolder => {
                            if let Some(parent) = image_path.parent() {
                                let printed_dir = parent.join("printed");
                                fs::create_dir_all(&printed_dir).ok();
                                if let Some(name) = image_path.file_name() {
                                    fs::rename(image_path, printed_dir.join(name)).ok();
                                }
                            }
                        }
                        PostFileAction::Delete => {
                            fs::remove_file(image_path).ok();
                        }
                        PostFileAction::Keep => {}
                    }
                }
                Err(e) => {
                    eprintln!("Print error for {}: {}", filename, e);
                    job_queue.update_status(job_id, JobStatus::Error, Some(e));
                    let _ = app.emit("job-updated", "");
                }
            }
        } else {
            // Not auto-print — leave as pending
            job_queue.add_job(job);
            let _ = app.emit("job-updated", "");
        }
    } else {
        // No matching preset — mark as skipped
        let mut job = PrintJob::new(
            filename.clone(),
            image_path.display().to_string(),
            None,
            None,
        );
        job.status = JobStatus::Skipped;
        job.error_message = Some(format!(
            "No preset matched{}",
            keyword
                .map(|k| format!(" for keyword '{}'", k))
                .unwrap_or_default()
        ));
        job_queue.add_job(job);
        let _ = app.emit("job-updated", "");
    }

    Ok(())
}

fn submit_print_job(app: &AppHandle, file_path: &Path, preset: &Preset) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let printer_id = preset
            .printer_id
            .as_ref()
            .ok_or_else(|| "No printer assigned to preset".to_string())?;
        submit_print_windows(file_path, printer_id, preset)
    }

    #[cfg(target_os = "macos")]
    {
        macos_printing::print_file(app, file_path, preset)
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let printer_id = preset
            .printer_id
            .as_ref()
            .ok_or_else(|| "No printer assigned to preset".to_string())?;
        submit_print_unix(file_path, printer_id, preset)
    }
}

// ─── Windows printing ────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn submit_print_windows(file_path: &Path, printer_id: &str, preset: &Preset) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // PowerShell single-quoted strings: only ' needs escaping as ''
    let file_str = file_path.display().to_string().replace('\'', "''");
    let printer_escaped = printer_id.replace('\'', "''");

    let script_content = build_print_script(&printer_escaped, &file_str, preset);

    // Write script to a unique temp file (avoid races with concurrent jobs)
    let script_path = std::env::temp_dir().join(format!(
        "helloprint_{}.ps1",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    fs::write(&script_path, &script_content)
        .map_err(|e| format!("Failed to write script: {}", e))?;

    eprintln!(
        "[HelloPrint] Submitting print job: printer={}, file={}",
        printer_id,
        file_path.display()
    );

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.display().to_string(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Failed to launch powershell: {}", e))?;

    // Clean up script
    fs::remove_file(&script_path).ok();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    eprintln!("[HelloPrint] PS stdout: {}", stdout.trim());
    if !stderr.is_empty() {
        eprintln!("[HelloPrint] PS stderr: {}", stderr.trim());
    }
    eprintln!("[HelloPrint] PS exit code: {:?}", output.status.code());

    if stdout.contains("SUCCESS") {
        return Ok(());
    }

    // If we got here, something went wrong
    let detail = if !stderr.is_empty() {
        stderr.trim().to_string()
    } else if !stdout.is_empty() {
        stdout.trim().to_string()
    } else {
        format!("Process exited with code {:?}", output.status.code())
    };

    Err(format!("Print failed: {}", detail))
}

#[cfg(target_os = "windows")]
fn build_print_script(printer_name: &str, file_path: &str, preset: &Preset) -> String {
    let copies = preset.copies;

    // Priority: DEVMODE blob > Print Ticket XML > basic PrintDocument
    let settings_block = if let Some(ref b64) = preset.devmode_base64 {
        // DEVMODE-direct path: decode stored blob and apply directly
        format!(
            r#"
    # Apply DEVMODE blob directly (captured from native driver dialog)
    $devModeB64 = '{b64}'
    $devModeBytes = [System.Convert]::FromBase64String($devModeB64)

    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Drawing.Printing;

public static class DevModeApplier {{
    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalAlloc(uint flags, UIntPtr size);
    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalLock(IntPtr hMem);
    [DllImport("kernel32.dll")]
    static extern bool GlobalUnlock(IntPtr hMem);

    public static void Apply(PrinterSettings ps, PageSettings page, byte[] dm) {{
        IntPtr hGlobal = GlobalAlloc(0x0042, (UIntPtr)dm.Length);
        IntPtr ptr = GlobalLock(hGlobal);
        Marshal.Copy(dm, 0, ptr, dm.Length);
        GlobalUnlock(hGlobal);
        ps.SetHdevmode(hGlobal);
        page.SetHdevmode(hGlobal);
    }}
}}
"@ -ReferencedAssemblies System.Drawing

    [DevModeApplier]::Apply($pd.PrinterSettings, $pd.DefaultPageSettings, $devModeBytes)
    $pd.PrinterSettings.Copies = {copies}
"#,
            b64 = b64,
            copies = copies,
        )
    } else if !preset.settings.is_empty() {
        // Print Ticket XML path (existing fallback)
        let mut settings_entries = String::new();
        for (key, value) in &preset.settings {
            let k = key.replace('\'', "''");
            let v = value.replace('\'', "''");
            settings_entries.push_str(&format!("    $settings['{}'] = '{}'\n", k, v));
        }

        format!(
            r#"
    # Apply custom print settings via Print Ticket
    Add-Type -AssemblyName System.Printing
    Add-Type -AssemblyName ReachFramework

    $settings = @{{}}
{settings_entries}

    $server = New-Object System.Printing.LocalPrintServer
    $queue = $server.GetPrintQueue($printerName)
    $ticket = $queue.DefaultPrintTicket

    # Read current ticket XML
    $xmlStream = $ticket.GetXmlStream()
    $reader = New-Object System.IO.StreamReader($xmlStream)
    $ticketXml = [xml]$reader.ReadToEnd()
    $reader.Close()
    $xmlStream.Close()

    $nsMgr = New-Object System.Xml.XmlNamespaceManager($ticketXml.NameTable)
    $nsMgr.AddNamespace('psf', 'http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework')

    # Register all namespaces from document root
    foreach ($attr in $ticketXml.DocumentElement.Attributes) {{
        if ($attr.Prefix -eq 'xmlns' -and $attr.LocalName -ne 'xmlns') {{
            try {{ $nsMgr.AddNamespace($attr.LocalName, $attr.Value) }} catch {{}}
        }}
    }}

    # Apply each setting to the ticket XML
    foreach ($key in $settings.Keys) {{
        $value = $settings[$key]
        $feature = $ticketXml.SelectSingleNode("//psf:Feature[@name='$key']", $nsMgr)

        if ($feature) {{
            # Remove existing options
            $existingOpts = $feature.SelectNodes('psf:Option', $nsMgr)
            foreach ($o in $existingOpts) {{ [void]$feature.RemoveChild($o) }}

            $newOpt = $ticketXml.CreateElement('psf', 'Option', 'http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework')
            $newOpt.SetAttribute('name', $value)
            [void]$feature.AppendChild($newOpt)
        }} else {{
            # Add new feature
            $newFeature = $ticketXml.CreateElement('psf', 'Feature', 'http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework')
            $newFeature.SetAttribute('name', $key)
            $newOpt = $ticketXml.CreateElement('psf', 'Option', 'http://schemas.microsoft.com/windows/2003/08/printing/printschemaframework')
            $newOpt.SetAttribute('name', $value)
            [void]$newFeature.AppendChild($newOpt)
            [void]$ticketXml.DocumentElement.AppendChild($newFeature)
        }}
    }}

    # Rebuild and validate the ticket
    $memStream = New-Object System.IO.MemoryStream
    $ticketXml.Save($memStream)
    $memStream.Position = 0
    $newTicket = New-Object System.Printing.PrintTicket($memStream)
    $memStream.Close()

    $validation = $queue.MergeAndValidatePrintTicket($queue.DefaultPrintTicket, $newTicket)
    $validatedTicket = $validation.ValidatedPrintTicket
    $validatedTicket.CopyCount = {copies}

    # Convert PrintTicket -> DEVMODE bytes
    Add-Type -AssemblyName System.Printing
    $converter = New-Object System.Printing.Interop.PrintTicketConverter(
        $printerName,
        $queue.ClientPrintSchemaVersion
    )
    $devModeBytes = $converter.ConvertPrintTicketToDevMode(
        $validatedTicket,
        [System.Printing.Interop.BaseDevModeType]::PrinterDefault
    )
    $converter.Dispose()
    $queue.Dispose()
    $server.Dispose()

    # Apply DEVMODE to PrintDocument
    # We need P/Invoke to allocate an HGLOBAL from the byte array
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Drawing.Printing;

public static class DevModeApplier {{
    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalAlloc(uint flags, UIntPtr size);
    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalLock(IntPtr hMem);
    [DllImport("kernel32.dll")]
    static extern bool GlobalUnlock(IntPtr hMem);

    public static void Apply(PrinterSettings ps, PageSettings page, byte[] dm) {{
        IntPtr hGlobal = GlobalAlloc(0x0042, (UIntPtr)dm.Length);
        IntPtr ptr = GlobalLock(hGlobal);
        Marshal.Copy(dm, 0, ptr, dm.Length);
        GlobalUnlock(hGlobal);
        ps.SetHdevmode(hGlobal);
        page.SetHdevmode(hGlobal);
    }}
}}
"@ -ReferencedAssemblies System.Drawing

    [DevModeApplier]::Apply($pd.PrinterSettings, $pd.DefaultPageSettings, $devModeBytes)
"#,
            settings_entries = settings_entries,
            copies = copies,
        )
    } else {
        String::new()
    };

    format!(
        r#"$ErrorActionPreference = 'Stop'
try {{
    Add-Type -AssemblyName System.Drawing

    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class PageGeometry {{
    [DllImport("gdi32.dll")]
    static extern int GetDeviceCaps(IntPtr hdc, int index);

    // PHYSICALWIDTH, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY, LOGPIXELSX, LOGPIXELSY
    public static int[] Read(IntPtr hdc) {{
        int[] idx = {{ 110, 111, 112, 113, 88, 90 }};
        int[] r = new int[idx.Length];
        for (int i = 0; i < idx.Length; i++) r[i] = GetDeviceCaps(hdc, idx[i]);
        return r;
    }}
}}
"@

    $printerName = '{printer_name}'
    $filePath = '{file_path}'

    $image = [System.Drawing.Image]::FromFile($filePath)

    $pd = New-Object System.Drawing.Printing.PrintDocument
    $pd.PrinterSettings.PrinterName = $printerName
    $pd.PrinterSettings.Copies = {copies}
{settings_block}
    $pd.add_PrintPage({{
        param($sender, $e)
        # Draw the image at the paper's size (no scaling logic — the paper size
        # and margins are already set by the DEVMODE / Print Ticket), centred on
        # the physical page. Some borderless modes (Epson "Retain Size") report a
        # page larger than the paper with the paper in the middle; drawing at
        # (0, 0) shifted those prints up and left. Graphics units are 1/100 inch
        # with the origin at the printable area, so subtract the physical offset.
        # When the page equals the paper this is the paper's own corner.
        $x = 0; $y = 0
        $hdc = $e.Graphics.GetHdc()
        try {{ $caps = [PageGeometry]::Read($hdc) }} finally {{ $e.Graphics.ReleaseHdc($hdc) }}
        if ($caps[4] -gt 0 -and $caps[5] -gt 0) {{
            $x = ($caps[0] / $caps[4] * 100 - $e.PageBounds.Width) / 2 - $caps[2] / $caps[4] * 100
            $y = ($caps[1] / $caps[5] * 100 - $e.PageBounds.Height) / 2 - $caps[3] / $caps[5] * 100
        }}
        $dest = New-Object System.Drawing.RectangleF([single]$x, [single]$y, $e.PageBounds.Width, $e.PageBounds.Height)
        $e.Graphics.DrawImage($image, $dest)
        $e.HasMorePages = $false
    }})

    $pd.Print()
    $image.Dispose()
    $pd.Dispose()

    Write-Output "SUCCESS"
}} catch {{
    Write-Error "PRINT_ERROR: $($_.Exception.GetType().FullName): $($_.Exception.Message)"
    Write-Error "Stack: $($_.ScriptStackTrace)"
    exit 1
}}
"#,
        printer_name = printer_name,
        file_path = file_path,
        copies = copies,
        settings_block = settings_block,
    )
}

// ─── macOS / Linux printing ──────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn submit_print_unix(file_path: &Path, printer_id: &str, preset: &Preset) -> Result<(), String> {
    use std::process::Command;

    let page_size_key = preset
        .settings
        .get("PageSize")
        .map(|s| s.as_str())
        .unwrap_or("Letter");
    let scaling_factor = get_cups_scaling_factor(printer_id, page_size_key);
    let has_vendor_quality = preset.settings.contains_key("EPIJ_Qual");

    // Step 1: Set all options as persistent user defaults via lpoptions.
    // This is the only reliable way to apply vendor PPD options (EPIJ_*).
    {
        let mut lpoptions = Command::new("lpoptions");
        lpoptions.arg("-p").arg(printer_id);
        let mut opt_log = Vec::new();

        for (key, value) in &preset.settings {
            if key == "print-scaling" {
                continue;
            }
            // Skip Resolution when vendor quality is set — it overrides quality
            if key == "Resolution" && has_vendor_quality {
                eprintln!(
                    "[HelloPrint] Skipping Resolution={} (EPIJ_Qual controls resolution)",
                    value
                );
                continue;
            }
            lpoptions.arg("-o").arg(format!("{}={}", key, value));
            opt_log.push(format!("{}={}", key, value));
        }

        // Force 720dpi when vendor quality is set
        if has_vendor_quality {
            lpoptions.arg("-o").arg("Resolution=720x720dpi");
            opt_log.push("Resolution=720x720dpi".to_string());
        }

        // Let Epson's vendor color matching handle sRGB→printer conversion.
        // Don't override EPIJ_OSColMat or EPIJ_CMat — the driver defaults work best
        // when submitting DeviceRGB content (it assumes sRGB input).

        eprintln!(
            "[HelloPrint] lpoptions -p {} {}",
            printer_id,
            opt_log
                .iter()
                .map(|o| format!("-o {}", o))
                .collect::<Vec<_>>()
                .join(" ")
        );

        let output = lpoptions
            .output()
            .map_err(|e| format!("lpoptions failed: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("[HelloPrint] lpoptions stderr: {}", stderr.trim());
        }
    }

    // Step 2: Wrap the image in a PDF with:
    // - Scaling compensation for cupsBorderlessScalingFactor
    // - Embedded sRGB ICC profile for correct color management
    // PDF submission is the only way to get precise scaling control on CUPS.
    let pdf_path = wrap_image_in_pdf(file_path, preset, scaling_factor)?;

    // Step 3: Submit the PDF via lp.
    // Quality/media options are already set via lpoptions (persistent defaults).
    let mut cmd = Command::new("lp");
    cmd.arg("-d").arg(printer_id);
    cmd.arg("-n").arg(preset.copies.to_string());
    cmd.arg(&pdf_path);

    eprintln!(
        "[HelloPrint] lp -d {} -n {} {}",
        printer_id,
        preset.copies,
        pdf_path.display()
    );

    let output = cmd.output().map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("[HelloPrint] lp stdout: {}", stdout.trim());
    if !stderr.is_empty() {
        eprintln!("[HelloPrint] lp stderr: {}", stderr.trim());
    }

    // Clean up temp PDF
    fs::remove_file(&pdf_path).ok();

    if !output.status.success() {
        return Err(format!("lp failed: {}", stderr));
    }

    Ok(())
}

/// Known paper sizes in points (1 point = 1/72 inch).
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn paper_size_points(page_size_key: &str) -> Option<(f32, f32)> {
    let base = page_size_key.split('.').next().unwrap_or(page_size_key);
    match base {
        "EPKG" | "4x6" => Some((288.0, 432.0)),
        "EPPhotoPaper2L" | "5x7" => Some((360.0, 504.0)),
        "EP8x10in" | "8x10" => Some((576.0, 720.0)),
        "EPPhotoPaperLRoll" | "3.5x5" => Some((252.0, 360.0)),
        "EPHiVision102x180" => Some((289.1, 510.2)),
        "Letter" => Some((612.0, 792.0)),
        "Legal" => Some((612.0, 1008.0)),
        "Executive" => Some((522.0, 756.0)),
        "A4" => Some((595.28, 841.89)),
        "A6" => Some((297.64, 419.53)),
        "EPHalfLetter" | "Statement" => Some((396.0, 612.0)),
        "Env10" => Some((297.0, 684.0)),
        _ => None,
    }
}

/// Wrap an image in a PDF with scaling compensation for borderless printing.
/// - JPEG: embedded as-is (DCTDecode passthrough) for full quality
/// - PNG/TIFF: decoded to RGB and compressed with FlateDecode
/// - DeviceRGB color space — Epson driver handles sRGB→printer conversion
/// - Image scaled down by 1/cupsBorderlessScalingFactor and centered
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn wrap_image_in_pdf(
    image_path: &Path,
    preset: &Preset,
    scaling_factor: f32,
) -> Result<PathBuf, String> {
    use lopdf::dictionary;
    use lopdf::{Document, Object, Stream};

    let page_size_key = preset
        .settings
        .get("PageSize")
        .map(|s| s.as_str())
        .unwrap_or("Letter");
    let (width_pt, height_pt) = paper_size_points(page_size_key).unwrap_or((288.0, 432.0));

    let orientation = preset
        .settings
        .get("orientation-requested")
        .map(|s| s.as_str());
    let (width_pt, height_pt) = match orientation {
        Some("4") | Some("5") => (height_pt, width_pt),
        _ => (width_pt, height_pt),
    };

    let img_width = width_pt / scaling_factor;
    let img_height = height_pt / scaling_factor;
    let offset_x = (width_pt - img_width) / 2.0;
    let offset_y = (height_pt - img_height) / 2.0;

    eprintln!(
        "[HelloPrint] PDF: page={:.1}x{:.1} pt ({:.2}x{:.2} in) PageSize={}",
        width_pt,
        height_pt,
        width_pt / 72.0,
        height_pt / 72.0,
        page_size_key
    );
    if scaling_factor != 1.0 {
        eprintln!("[HelloPrint] Scale compensation: factor={:.4}, image={:.1}x{:.1}, offset=({:.1},{:.1})",
            scaling_factor, img_width, img_height, offset_x, offset_y);
    }

    let image_bytes = fs::read(image_path).map_err(|e| format!("Failed to read image: {}", e))?;
    let img = ::image::load_from_memory(&image_bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    let (img_w, img_h) = ::image::GenericImageView::dimensions(&img);

    eprintln!(
        "[HelloPrint] Image: {}x{} px, {} bytes",
        img_w,
        img_h,
        image_bytes.len()
    );

    let ext = image_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let (stream_bytes, filter) = if ext == "jpg" || ext == "jpeg" {
        eprintln!("[HelloPrint] JPEG passthrough: {} bytes", image_bytes.len());
        (image_bytes, "DCTDecode")
    } else {
        let rgb = img.to_rgb8();
        let raw_pixels = rgb.into_raw();
        let compressed = {
            use flate2::write::ZlibEncoder;
            use flate2::Compression;
            use std::io::Write;
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::best());
            enc.write_all(&raw_pixels)
                .map_err(|e| format!("Compression error: {}", e))?;
            enc.finish()
                .map_err(|e| format!("Compression error: {}", e))?
        };
        eprintln!(
            "[HelloPrint] FlateDecode: {} → {} bytes",
            raw_pixels.len(),
            compressed.len()
        );
        (compressed, "FlateDecode")
    };

    let mut doc = Document::with_version("1.4");

    let image_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => img_w as i64,
            "Height" => img_h as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8_i64,
            "Filter" => filter,
        },
        stream_bytes,
    );
    let image_id = doc.add_object(image_stream);

    let content = format!(
        "q\n{:.4} 0 0 {:.4} {:.4} {:.4} cm\n/Img1 Do\nQ\n",
        img_width, img_height, offset_x, offset_y,
    );
    let content_stream = Stream::new(
        dictionary! { "Length" => content.len() as i64 },
        content.into_bytes(),
    );
    let content_id = doc.add_object(content_stream);

    let resources = dictionary! {
        "XObject" => dictionary! { "Img1" => image_id },
    };

    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(width_pt), Object::Real(height_pt)],
        "Contents" => content_id,
        "Resources" => resources,
    });

    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    });

    if let Ok(page_obj) = doc.get_object_mut(page_id) {
        if let Object::Dictionary(ref mut dict) = page_obj {
            dict.set("Parent", pages_id);
        }
    }

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let pdf_path = std::env::temp_dir().join(format!(
        "helloprint_{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    doc.save(&pdf_path)
        .map_err(|e| format!("Failed to save PDF: {}", e))?;

    eprintln!(
        "[HelloPrint] Generated PDF: {} ({:.1} KB)",
        pdf_path.display(),
        fs::metadata(&pdf_path)
            .map(|m| m.len() as f64 / 1024.0)
            .unwrap_or(0.0)
    );

    Ok(pdf_path)
}

/// Read the system PPD for a CUPS printer and extract `cupsBorderlessScalingFactor`
/// for the given page size keyword. Returns 1.0 if no factor is found.
///
/// The PPD lives at `/private/etc/cups/ppd/<printer_id>.ppd` on macOS.
/// We look for lines like:
///   *PageSize EPKG.NMgn/...: "<</PageSize[...]/cupsBorderlessScalingFactor 1.06>>..."
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub fn get_cups_scaling_factor(printer_id: &str, page_size_key: &str) -> f32 {
    let sanitized_id: String = printer_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ppd_path = format!("/private/etc/cups/ppd/{}.ppd", sanitized_id);
    let content = match fs::read_to_string(&ppd_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[HelloPrint] Could not read PPD at {}: {}", ppd_path, e);
            return 1.0;
        }
    };

    // Look for the *PageSize line matching our key
    // Format: *PageSize KEY/Label: "<<...cupsBorderlessScalingFactor VALUE...>>"
    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("*PageSize ") {
            continue;
        }
        // Extract the key part: "*PageSize KEY/..." or "*PageSize KEY:"
        let rest = &line[10..]; // skip "*PageSize "
        let key_end = rest
            .find('/')
            .unwrap_or_else(|| rest.find(':').unwrap_or(rest.len()));
        let key = rest[..key_end].trim();
        if key != page_size_key {
            continue;
        }

        // Found the matching line — look for cupsBorderlessScalingFactor
        if let Some(pos) = line.find("cupsBorderlessScalingFactor") {
            let after = &line[pos + "cupsBorderlessScalingFactor".len()..];
            let after = after.trim_start();
            // Parse the float value (may be followed by >> or other chars)
            let end = after
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(after.len());
            if let Ok(factor) = after[..end].parse::<f32>() {
                eprintln!(
                    "[HelloPrint] PPD cupsBorderlessScalingFactor for {}: {}",
                    page_size_key, factor
                );
                return factor;
            }
        }

        // Found the page size line but no scaling factor
        break;
    }

    eprintln!(
        "[HelloPrint] No cupsBorderlessScalingFactor found for {} in PPD",
        page_size_key
    );
    1.0
}

/// Search the PPD for a borderless scaling factor matching a user-friendly keyword
/// (e.g. "4x6", "kg", "5x7"). Searches PageSize label text for the keyword pattern.
#[cfg(not(target_os = "windows"))]
pub fn get_cups_scaling_factor_by_keyword(printer_id: &str, keyword: &str) -> f32 {
    // CUPS PPD filenames use underscores instead of spaces
    let sanitized_id: String = printer_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ppd_path = format!("/private/etc/cups/ppd/{}.ppd", sanitized_id);
    let content = match fs::read_to_string(&ppd_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[HelloPrint] Could not read PPD at {}: {}", ppd_path, e);
            return 1.0;
        }
    };

    let kw = keyword.trim().to_lowercase();
    // Build search patterns for the keyword
    // "4x6" → look for "4x6" or "4 x 6" or "KG" in the label
    // "kg" → look for "KG" in the label
    let patterns: Vec<String> = match kw.as_str() {
        "4x6" | "kg" | "4r" => vec!["4x6".into(), "4 x 6".into(), "kg".into()],
        "5x7" | "5r" | "2l" => vec!["5x7".into(), "5 x 7".into()],
        "8x10" => vec!["8x10".into(), "8 x 10".into()],
        "3.5x5" | "l" => vec!["3.5x5".into(), "3.5 x 5".into()],
        "letter" | "8.5x11" => vec!["letter".into(), "8.5x11".into()],
        "a4" => vec!["a4".into()],
        "a6" => vec!["a6".into()],
        other => vec![other.to_string()],
    };

    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with("*PageSize ") {
            continue;
        }
        // Only consider borderless entries (must have cupsBorderlessScalingFactor)
        if !line.contains("cupsBorderlessScalingFactor") {
            continue;
        }

        let line_lower = line.to_lowercase();
        let matches = patterns.iter().any(|p| line_lower.contains(p));
        if !matches {
            continue;
        }

        // Extract the scaling factor
        if let Some(pos) = line.find("cupsBorderlessScalingFactor") {
            let after = &line[pos + "cupsBorderlessScalingFactor".len()..];
            let after = after.trim_start();
            let end = after
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(after.len());
            if let Ok(factor) = after[..end].parse::<f32>() {
                eprintln!(
                    "[HelloPrint] PPD cupsBorderlessScalingFactor for keyword '{}': {}",
                    keyword, factor
                );
                return factor;
            }
        }
    }

    eprintln!(
        "[HelloPrint] No cupsBorderlessScalingFactor found for keyword '{}' in PPD",
        keyword
    );
    1.0
}

pub fn stop_watcher_inner(watcher_state: &WatcherState) {
    if let Ok(mut w) = watcher_state.watcher.lock() {
        *w = None;
    }
    if let Ok(mut s) = watcher_state.status.lock() {
        *s = WatcherStatus::Idle;
    }
}
