use std::sync::Arc;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::jobs::{JobQueueState, JobStatus, PrintJob};
#[cfg(target_os = "macos")]
use crate::macos_printing::{self, MacOSPrintConfiguration};
use crate::models::{AppConfig, Preset};
#[cfg(target_os = "windows")]
use crate::printer_setup;
use crate::printer_setup::ExpansionCheck;
use crate::printing::{self, PrinterCapabilities, PrinterInfo};
use crate::storage::StorageState;
use crate::watcher::{self, WatcherState, WatcherStatus};

/// Opens the folder holding the app's log files, for sending with a bug report.
#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;

    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_platform() -> String {
    if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else {
        "linux".to_string()
    }
}

/// Opens the printer driver's settings dialog and returns the DEVMODE as
/// base64. Starts from `seed_devmode_base64` when given (editing a preset's
/// saved settings), otherwise from the printer's defaults pre-filled with the
/// keyword's paper size, borderless and portrait.
///
/// Async so the dialog never blocks the main thread: a sync command would
/// freeze the app window until the dialog closed.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn open_printer_dialog(
    window: tauri::WebviewWindow,
    printer_id: String,
    paper_size_keyword: Option<String>,
    seed_devmode_base64: Option<String>,
) -> Result<String, String> {
    let owner_hwnd = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    tauri::async_runtime::spawn_blocking(move || {
        printer_setup::open_printer_dialog(
            &printer_id,
            paper_size_keyword.as_deref(),
            seed_devmode_base64.as_deref(),
            owner_hwnd,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn open_printer_dialog(
    _printer_id: String,
    _paper_size_keyword: Option<String>,
    _seed_devmode_base64: Option<String>,
) -> Result<String, String> {
    Err("Printer dialog is only available on Windows".to_string())
}

/// Checks captured printer settings for borderless expansion without printing.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn check_expansion(
    printer_id: String,
    devmode_base64: String,
) -> Result<ExpansionCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        printer_setup::check_expansion(&printer_id, &devmode_base64)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn check_expansion(
    _printer_id: String,
    _devmode_base64: String,
) -> Result<ExpansionCheck, String> {
    Err("Expansion check is only available on Windows".to_string())
}

/// Prints a 50 mm measurement sheet with the given (possibly unsaved) settings.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn print_test_sheet(
    printer_id: String,
    paper_size_keyword: String,
    devmode_base64: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        printer_setup::print_test_sheet(&printer_id, &paper_size_keyword, devmode_base64.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn print_test_sheet(
    _printer_id: String,
    _paper_size_keyword: String,
    _devmode_base64: Option<String>,
) -> Result<(), String> {
    Err("Test sheet printing is only available on Windows".to_string())
}

#[tauri::command]
pub fn get_config(state: State<Arc<StorageState>>) -> Result<AppConfig, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub fn save_config(state: State<Arc<StorageState>>, config: AppConfig) -> Result<(), String> {
    state.save_config(&config)?;
    let mut current = state.config.lock().map_err(|e| e.to_string())?;
    *current = config;
    Ok(())
}

#[tauri::command]
pub fn list_presets(state: State<Arc<StorageState>>) -> Result<Vec<Preset>, String> {
    let presets = state.presets.lock().map_err(|e| e.to_string())?;
    Ok(presets.clone())
}

#[tauri::command]
pub fn create_preset(
    state: State<Arc<StorageState>>,
    name: String,
    paper_size_keyword: String,
) -> Result<Preset, String> {
    let preset = Preset::new(name, paper_size_keyword);
    let mut presets = state.presets.lock().map_err(|e| e.to_string())?;
    presets.push(preset.clone());
    state.save_presets(&presets)?;
    Ok(preset)
}

#[tauri::command]
pub fn update_preset(state: State<Arc<StorageState>>, preset: Preset) -> Result<Preset, String> {
    let mut presets = state.presets.lock().map_err(|e| e.to_string())?;
    let idx = presets
        .iter()
        .position(|p| p.id == preset.id)
        .ok_or_else(|| format!("Preset {} not found", preset.id))?;
    presets[idx] = preset.clone();
    state.save_presets(&presets)?;
    Ok(preset)
}

#[tauri::command]
pub fn delete_preset(state: State<Arc<StorageState>>, id: Uuid) -> Result<(), String> {
    let mut presets = state.presets.lock().map_err(|e| e.to_string())?;
    let idx = presets
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("Preset {} not found", id))?;
    presets.remove(idx);
    state.save_presets(&presets)?;
    Ok(())
}

#[tauri::command]
pub fn list_printers(app_handle: AppHandle) -> Vec<PrinterInfo> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(printers) = macos_printing::list_printers(&app_handle) {
            return printers;
        }
    }

    printing::discover_printers()
}

#[tauri::command]
pub fn get_printer_capabilities(printer_id: String) -> PrinterCapabilities {
    printing::get_printer_capabilities(&printer_id)
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn configure_macos_printer(
    app_handle: AppHandle,
    printer_id: Option<String>,
    paper_size_keyword: Option<String>,
) -> Result<MacOSPrintConfiguration, String> {
    macos_printing::configure_printer(
        &app_handle,
        printer_id.as_deref(),
        paper_size_keyword.as_deref(),
    )
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn configure_macos_printer(
    _app_handle: AppHandle,
    _printer_id: Option<String>,
    _paper_size_keyword: Option<String>,
) -> Result<String, String> {
    Err("macOS native printer configuration is only available on macOS".to_string())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_borderless_scale_factor(printer_id: String, paper_size_keyword: String) -> f64 {
    crate::watcher::get_cups_scaling_factor_by_keyword(&printer_id, &paper_size_keyword) as f64
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_borderless_scale_factor(_printer_id: String, _paper_size_keyword: String) -> f64 {
    1.0
}

#[tauri::command]
pub fn start_watcher(
    app_handle: AppHandle,
    watch_folder: String,
    watcher_state: State<Arc<WatcherState>>,
    storage_state: State<Arc<StorageState>>,
    job_queue: State<Arc<JobQueueState>>,
) -> Result<(), String> {
    watcher::start_watcher(
        app_handle,
        watch_folder,
        (*watcher_state).clone(),
        (*storage_state).clone(),
        (*job_queue).clone(),
    )
}

#[tauri::command]
pub fn stop_watcher(watcher_state: State<Arc<WatcherState>>) -> Result<(), String> {
    watcher::stop_watcher_inner(&watcher_state);
    Ok(())
}

#[tauri::command]
pub fn get_watcher_status(
    watcher_state: State<Arc<WatcherState>>,
) -> Result<WatcherStatus, String> {
    let status = watcher_state.status.lock().map_err(|e| e.to_string())?;
    Ok(status.clone())
}

#[tauri::command]
pub fn list_jobs(queue: State<Arc<JobQueueState>>) -> Result<Vec<PrintJob>, String> {
    let jobs = queue.jobs.lock().map_err(|e| e.to_string())?;
    Ok(jobs.clone())
}

#[tauri::command]
pub fn cancel_job(queue: State<Arc<JobQueueState>>, id: Uuid) -> Result<(), String> {
    let mut jobs = queue.jobs.lock().map_err(|e| e.to_string())?;
    if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
        if job.status == JobStatus::Pending {
            job.status = JobStatus::Error;
            job.error_message = Some("Cancelled by user".to_string());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn retry_job(queue: State<Arc<JobQueueState>>, id: Uuid) -> Result<(), String> {
    let mut jobs = queue.jobs.lock().map_err(|e| e.to_string())?;
    if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
        if job.status == JobStatus::Error {
            job.status = JobStatus::Pending;
            job.error_message = None;
            job.completed_at = None;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn reprint_job(queue: State<Arc<JobQueueState>>, id: Uuid) -> Result<(), String> {
    let jobs = queue.jobs.lock().map_err(|e| e.to_string())?;
    let original = jobs
        .iter()
        .find(|j| j.id == id)
        .ok_or_else(|| "Job not found".to_string())?
        .clone();
    drop(jobs);

    let new_job = PrintJob::new(
        original.filename,
        original.file_path,
        original.preset_id,
        original.preset_name,
    );
    queue.add_job(new_job);
    Ok(())
}
