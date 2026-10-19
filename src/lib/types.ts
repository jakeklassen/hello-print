export type PostFileAction = "delete" | "move_to_subfolder" | "keep";

export interface AppConfig {
  watch_folder: string | null;
  minimize_to_tray: boolean;
  default_preset_id: string | null;
  post_print_action: PostFileAction;
  post_zip_action: PostFileAction;
}

/** Maps printer option key → selected value */
export type PrintSettings = Record<string, string>;

export interface Preset {
  id: string;
  name: string;
  printer_id: string | null;
  paper_size_keyword: string;
  settings: PrintSettings;
  copies: number;
  auto_print: boolean;
  scale_compensation: number;
  devmode_base64: string | null;
  macos_print_info_base64: string | null;
  macos_page_format_base64: string | null;
  macos_print_settings_base64: string | null;
  macos_printer_name: string | null;
  macos_page_width_points: number | null;
  macos_page_height_points: number | null;
  macos_size_compensation_mm: number | null;
  created_at: string;
  updated_at: string;
}

export type ExpansionStatus = "no_expansion_detected" | "expanding" | "unknown";

/** Result of checking captured Windows printer settings for borderless expansion. */
export interface ExpansionCheck {
  status: ExpansionStatus;
  logical_dpi: number;
  print_quality_dpi: number | null;
  /** logical ÷ print quality, e.g. 1.0528 for a 5.3% enlargement */
  factor: number | null;
  page_mm: [number, number] | null;
  paper_mm: [number, number] | null;
}

/** A named paper size with a reference test sheet (portrait). */
export interface PaperSize {
  slug: string;
  /** Filename keyword that routes to this size, if any */
  keyword: string | null;
  label: string;
  width_mm: number;
  height_mm: number;
}

export interface MacOSPrintConfiguration {
  printer_name: string;
  print_info_base64: string;
  page_format_base64: string;
  print_settings_base64: string;
  page_width_points: number;
  page_height_points: number;
}

export interface PrinterInfo {
  id: string;
  name: string;
  driver: string;
  is_default: boolean;
  is_online: boolean;
}

export interface PrinterOptionChoice {
  value: string;
  label: string;
}

export interface PrinterOption {
  key: string;
  label: string;
  choices: PrinterOptionChoice[];
  default_choice: string | null;
}

export interface PrinterCapabilities {
  options: PrinterOption[];
}

export type JobStatus =
  | "pending"
  | "printing"
  | "complete"
  | "error"
  | "skipped";

export interface PrintJob {
  id: string;
  filename: string;
  file_path: string;
  preset_id: string | null;
  preset_name: string | null;
  status: JobStatus;
  error_message: string | null;
  created_at: string;
  completed_at: string | null;
}
