import { useEffect, useState } from "react";
import {
  Settings2,
  X,
  Check,
  CircleCheck,
  CircleQuestionMark,
  Printer,
  TriangleAlert,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { PrinterSelector } from "@/components/printer-selector";
import { CapabilitiesForm } from "@/components/capabilities-form";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  checkExpansion,
  configureMacosPrinter,
  getPlatform,
  openPrinterDialog,
  printTestSheet,
} from "@/lib/api";
import type { ExpansionCheck, Preset, PrintSettings } from "@/lib/types";

const DEFAULT_SETTINGS: PrintSettings = {};

export interface PresetFormData {
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
}

interface PresetFormProps {
  preset?: Preset;
  onSave: (data: PresetFormData) => void;
  onCancel: () => void;
}

export function PresetForm({ preset, onSave, onCancel }: PresetFormProps) {
  const deriveMacosCompensationMm = (value?: Preset) => {
    if (value?.macos_size_compensation_mm != null) {
      return value.macos_size_compensation_mm;
    }

    if (
      value?.macos_page_width_points &&
      value.scale_compensation > 0 &&
      value.scale_compensation < 0.999
    ) {
      const pageWidthMm = (value.macos_page_width_points / 72) * 25.4;
      return pageWidthMm * value.scale_compensation - pageWidthMm;
    }

    return 0;
  };

  const [name, setName] = useState(preset?.name ?? "");
  const [printerId, setPrinterId] = useState<string | null>(
    preset?.printer_id ?? null,
  );
  const [paperSizeKeyword, setPaperSizeKeyword] = useState(
    preset?.paper_size_keyword ?? "",
  );
  const [settings, setSettings] = useState<PrintSettings>(
    preset?.settings ?? DEFAULT_SETTINGS,
  );
  const [copies, setCopies] = useState(preset?.copies ?? 1);
  const [autoPrint, setAutoPrint] = useState(preset?.auto_print ?? true);
  const [devmodeBase64, setDevmodeBase64] = useState<string | null>(
    preset?.devmode_base64 ?? null,
  );
  const [macosPrintInfoBase64, setMacosPrintInfoBase64] = useState<
    string | null
  >(preset?.macos_print_info_base64 ?? null);
  const [macosPrinterName, setMacosPrinterName] = useState<string | null>(
    preset?.macos_printer_name ?? null,
  );
  const [macosPageFormatBase64, setMacosPageFormatBase64] = useState<
    string | null
  >(preset?.macos_page_format_base64 ?? null);
  const [macosPrintSettingsBase64, setMacosPrintSettingsBase64] = useState<
    string | null
  >(preset?.macos_print_settings_base64 ?? null);
  const [macosPageWidthPoints, setMacosPageWidthPoints] = useState<
    number | null
  >(preset?.macos_page_width_points ?? null);
  const [macosPageHeightPoints, setMacosPageHeightPoints] = useState<
    number | null
  >(preset?.macos_page_height_points ?? null);
  const [macosSizeCompensationMm, setMacosSizeCompensationMm] =
    useState<number>(deriveMacosCompensationMm(preset));
  const [scaleCompensation] = useState(preset?.scale_compensation ?? 1.0);
  const [platform, setPlatform] = useState<string | null>(null);
  const [dialogLoading, setDialogLoading] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [macosConfigError, setMacosConfigError] = useState<string | null>(null);
  const [expansionCheck, setExpansionCheck] = useState<ExpansionCheck | null>(
    null,
  );
  const [checkingExpansion, setCheckingExpansion] = useState(false);
  const [testPrintStatus, setTestPrintStatus] = useState<
    "idle" | "sending" | "sent"
  >("idle");
  const [testPrintError, setTestPrintError] = useState<string | null>(null);
  const [confirmTestPrint, setConfirmTestPrint] = useState(false);

  useEffect(() => {
    getPlatform().then(setPlatform);
  }, []);

  const runExpansionCheck = async (printer: string, b64: string) => {
    setCheckingExpansion(true);
    setExpansionCheck(null);

    try {
      setExpansionCheck(await checkExpansion(printer, b64));
    } catch (err) {
      console.error("Expansion check error:", err);
    } finally {
      setCheckingExpansion(false);
    }
  };

  // Re-check saved settings when editing a preset: a driver update or changed
  // printer defaults can turn expansion back on.
  useEffect(() => {
    if (platform === "windows" && printerId && devmodeBase64) {
      runExpansionCheck(printerId, devmodeBase64);
    }
    // Only on load; later changes are checked when the dialog closes.
  }, [platform]);

  const platformLoaded = platform !== null;
  const isWindows = platform === "windows";
  const isMacos = platform === "macos";
  // Without captured driver settings a Windows preset prints with whatever the
  // printer defaults are, which is rarely true size.
  const needsWindowsConfig = isWindows && !devmodeBase64;
  const hasMacosNativeConfig = Boolean(
    macosPrintInfoBase64 &&
    macosPageFormatBase64 &&
    macosPrintSettingsBase64 &&
    macosPrinterName,
  );

  const handleOpenDialog = async () => {
    if (!printerId) {
      return;
    }

    setDialogLoading(true);
    setDialogError(null);

    try {
      // Edit the saved settings when there are some; otherwise the dialog
      // opens pre-filled for the paper size.
      const b64 = await openPrinterDialog(
        printerId,
        paperSizeKeyword || null,
        devmodeBase64,
      );
      setDevmodeBase64(b64);
      setTestPrintStatus("idle");
      await runExpansionCheck(printerId, b64);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);

      if (!msg.includes("cancelled")) {
        console.error("Printer dialog error:", msg);
        setDialogError(msg);
      }
    } finally {
      setDialogLoading(false);
    }
  };

  const handleTestPrint = async () => {
    if (!printerId || !paperSizeKeyword) {
      return;
    }

    setTestPrintStatus("sending");
    setTestPrintError(null);

    try {
      await printTestSheet(printerId, paperSizeKeyword, devmodeBase64);
      setTestPrintStatus("sent");
    } catch (err) {
      setTestPrintStatus("idle");
      setTestPrintError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleConfigureMacos = async () => {
    setDialogLoading(true);
    setMacosConfigError(null);

    try {
      const config = await configureMacosPrinter(
        printerId,
        paperSizeKeyword || null,
      );

      if (
        !config.print_info_base64 ||
        !config.page_format_base64 ||
        !config.print_settings_base64 ||
        !config.printer_name
      ) {
        const missing = [
          !config.printer_name ? "printer_name" : null,
          !config.print_info_base64 ? "print_info_base64" : null,
          !config.page_format_base64 ? "page_format_base64" : null,
          !config.print_settings_base64 ? "print_settings_base64" : null,
        ]
          .filter(Boolean)
          .join(", ");
        setMacosConfigError(
          `macOS configure returned incomplete data: ${missing}`,
        );
        return;
      }

      console.log("macOS printer configuration captured", {
        printer_name: config.printer_name,
        print_info_length: config.print_info_base64.length,
        page_format_length: config.page_format_base64.length,
        print_settings_length: config.print_settings_base64.length,
      });
      setPrinterId(config.printer_name);
      setMacosPrinterName(config.printer_name);
      setMacosPrintInfoBase64(config.print_info_base64);
      setMacosPageFormatBase64(config.page_format_base64);
      setMacosPrintSettingsBase64(config.print_settings_base64);
      setMacosPageWidthPoints(config.page_width_points);
      setMacosPageHeightPoints(config.page_height_points);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);

      if (!msg.includes("cancelled")) {
        console.error("macOS printer configuration error:", msg);
        setMacosConfigError(msg);
      }
    } finally {
      setDialogLoading(false);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    if (needsWindowsConfig) {
      return;
    }

    if (isMacos && !hasMacosNativeConfig) {
      setMacosConfigError(
        "Run Configure Printer Settings and save the native macOS printer configuration before saving this preset.",
      );
      return;
    }

    onSave({
      name,
      printer_id: printerId,
      paper_size_keyword: paperSizeKeyword,
      settings,
      copies,
      auto_print: autoPrint,
      scale_compensation: isMacos ? 1.0 : scaleCompensation,
      devmode_base64: devmodeBase64,
      macos_print_info_base64: macosPrintInfoBase64,
      macos_page_format_base64: macosPageFormatBase64,
      macos_print_settings_base64: macosPrintSettingsBase64,
      macos_printer_name: macosPrinterName,
      macos_page_width_points: macosPageWidthPoints,
      macos_page_height_points: macosPageHeightPoints,
      macos_size_compensation_mm: isMacos ? macosSizeCompensationMm : null,
    });
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-6">
      <div className="grid gap-1.5">
        <Label htmlFor="preset-name">Preset Name</Label>
        <Input
          id="preset-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g., 4x6 Photo Magnets"
          required
        />
      </div>

      <div className="grid gap-1.5">
        <Label>Printer</Label>
        <PrinterSelector value={printerId} onChange={setPrinterId} />
      </div>

      <div className="grid gap-1.5">
        <Label htmlFor="keyword">Paper Size Keyword</Label>
        <Input
          id="keyword"
          value={paperSizeKeyword}
          onChange={(e) => {
            setPaperSizeKeyword(e.target.value);

            if (isMacos) {
              setMacosConfigError(null);
            }
          }}
          placeholder="e.g., 4x6, A4, letter"
          required
        />
        <p className="text-xs text-muted-foreground">
          Files with this keyword in their name will route to this preset.
        </p>
      </div>

      {isWindows && (
        <div className="grid gap-1.5">
          <Label>Driver Settings (Windows)</Label>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!printerId || dialogLoading}
              onClick={handleOpenDialog}
            >
              <Settings2 className="mr-2 h-4 w-4" />
              {dialogLoading
                ? "Waiting for dialog..."
                : "Configure Printer Settings"}
            </Button>
            {devmodeBase64 ? (
              <div className="flex items-center gap-1.5">
                <span className="flex items-center gap-1 text-xs text-green-600">
                  <Check className="h-3.5 w-3.5" />
                  Configured
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  onClick={() => {
                    setDevmodeBase64(null);
                    setExpansionCheck(null);
                  }}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </div>
            ) : (
              <span className="text-xs text-muted-foreground">
                Not configured
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            Opens the printer&apos;s own settings, set up for the paper size
            above. For true-size borderless prints, turn off borderless
            enlargement there (Epson: Borderless &rarr; Settings&hellip; &rarr;
            Retain Size).
          </p>
          {dialogError && (
            <p className="text-xs text-destructive">{dialogError}</p>
          )}

          <ExpansionNotice
            check={expansionCheck}
            checking={checkingExpansion}
            fixDisabled={dialogLoading}
            onFix={handleOpenDialog}
          />

          <div className="flex items-center gap-2 pt-1">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={
                !printerId ||
                !paperSizeKeyword ||
                dialogLoading ||
                testPrintStatus === "sending"
              }
              onClick={() => setConfirmTestPrint(true)}
            >
              <Printer className="mr-2 h-4 w-4" />
              {testPrintStatus === "sending"
                ? "Sending test sheet..."
                : "Print Test Sheet"}
            </Button>
            <Dialog open={confirmTestPrint} onOpenChange={setConfirmTestPrint}>
              <DialogContent className="max-w-sm">
                <DialogHeader>
                  <DialogTitle>Print a test sheet?</DialogTitle>
                  <DialogDescription>
                    This prints one {paperSizeKeyword} sheet on {printerId} with
                    the current settings. Load paper before printing.
                  </DialogDescription>
                </DialogHeader>
                <DialogFooter>
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => setConfirmTestPrint(false)}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    onClick={() => {
                      setConfirmTestPrint(false);
                      handleTestPrint();
                    }}
                  >
                    <Printer className="mr-2 h-4 w-4" />
                    Print
                  </Button>
                </DialogFooter>
              </DialogContent>
            </Dialog>
            {testPrintStatus === "sent" && (
              <span className="text-xs text-muted-foreground">
                Sent. The centre square should measure exactly 50 &times; 50 mm,
                and the blue band should show on every edge.
              </span>
            )}
          </div>
          {testPrintError && (
            <p className="text-xs text-destructive">{testPrintError}</p>
          )}
        </div>
      )}

      {isMacos && (
        <div className="grid gap-1.5">
          <Label>Printer Settings (macOS)</Label>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={dialogLoading}
              onClick={handleConfigureMacos}
            >
              <Settings2 className="mr-2 h-4 w-4" />
              {dialogLoading
                ? "Opening print dialog..."
                : "Configure Printer Settings"}
            </Button>
            {hasMacosNativeConfig ? (
              <div className="flex items-center gap-1.5">
                <span className="flex items-center gap-1 text-xs text-green-600">
                  <Check className="h-3.5 w-3.5" />
                  Configured
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  onClick={() => {
                    setMacosPrintInfoBase64(null);
                    setMacosPageFormatBase64(null);
                    setMacosPrintSettingsBase64(null);
                    setMacosPrinterName(null);
                  }}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </div>
            ) : (
              <span className="text-xs text-muted-foreground">
                Not configured
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            Opens the native macOS print dialog using a real sample page when
            available and stores the resulting print configuration for headless
            reuse.
          </p>
          <p className="text-xs text-muted-foreground">
            For exact template positioning, prefer custom page sizes in macOS
            Page Setup instead of vendor borderless paper presets.
          </p>
          {macosConfigError && (
            <p className="text-xs text-destructive">{macosConfigError}</p>
          )}
          {macosPrinterName && (
            <p className="text-xs text-muted-foreground">
              Captured printer: {macosPrinterName}
            </p>
          )}
          {hasMacosNativeConfig && (
            <p className="text-xs text-muted-foreground">
              Native config captured: printInfo{" "}
              {macosPrintInfoBase64?.length ?? 0} chars, pageFormat{" "}
              {macosPageFormatBase64?.length ?? 0} chars, printSettings{" "}
              {macosPrintSettingsBase64?.length ?? 0} chars.
            </p>
          )}
        </div>
      )}

      {isMacos && macosPrintInfoBase64 && (
        <div className="grid gap-1.5">
          <Label htmlFor="macos-size-compensation">
            Size Compensation (mm)
          </Label>
          <Input
            id="macos-size-compensation"
            type="number"
            step={0.1}
            className="no-spinners"
            value={macosSizeCompensationMm}
            onChange={(e) => {
              const value = Number.parseFloat(e.target.value);
              setMacosSizeCompensationMm(Number.isFinite(value) ? value : 0);
            }}
          />
          <p className="text-xs text-muted-foreground">
            Negative values shrink the printed content. Positive values enlarge
            it. Use this only when the measured print size needs correction.
          </p>
        </div>
      )}

      {platformLoaded && !isWindows && !isMacos && (
        <div className="space-y-3">
          <Label className="text-sm font-medium">Print Settings</Label>
          <CapabilitiesForm
            printerId={printerId}
            settings={settings}
            onChange={setSettings}
          />
        </div>
      )}

      <div className="grid gap-1.5">
        <Label htmlFor="copies">Copies</Label>
        <Input
          id="copies"
          type="number"
          min={1}
          max={100}
          className="no-spinners"
          value={copies}
          onChange={(e) => setCopies(parseInt(e.target.value) || 1)}
        />
      </div>

      <div className="flex items-center gap-3">
        <Switch
          id="auto-print"
          checked={autoPrint}
          onCheckedChange={setAutoPrint}
        />
        <Label htmlFor="auto-print">Auto-print (no confirmation)</Label>
      </div>

      <div className="flex items-center justify-end gap-2">
        {needsWindowsConfig && (
          <p className="mr-auto text-xs text-muted-foreground">
            Configure printer settings to save this preset.
          </p>
        )}
        <Button type="button" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={needsWindowsConfig}>
          {preset ? "Save Changes" : "Create Preset"}
        </Button>
      </div>
    </form>
  );
}

interface ExpansionNoticeProps {
  check: ExpansionCheck | null;
  checking: boolean;
  fixDisabled: boolean;
  onFix: () => void;
}

/** Shows whether the captured printer settings enlarge borderless prints. */
function ExpansionNotice({
  check,
  checking,
  fixDisabled,
  onFix,
}: ExpansionNoticeProps) {
  if (checking) {
    return (
      <p className="text-xs text-muted-foreground">
        Checking for borderless enlargement...
      </p>
    );
  }

  if (!check) {
    return null;
  }

  if (check.status === "no_expansion_detected") {
    return (
      <p className="flex items-start gap-1.5 text-xs text-green-600">
        <CircleCheck className="mt-px h-3.5 w-3.5 shrink-0" />
        No enlargement detected. Prints should come out at true size; confirm
        with a test sheet.
      </p>
    );
  }

  if (check.status === "printer_decides") {
    return (
      <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
        <CircleQuestionMark className="mt-px h-3.5 w-3.5 shrink-0" />
        This printer uses Windows&apos; generic driver, which leaves borderless
        enlargement to the printer itself, so Hello Print can&apos;t check it.
        Print a test sheet and measure the 50 mm square. If it prints larger,
        install the manufacturer&apos;s driver, which usually has a setting to
        turn enlargement off.
      </p>
    );
  }

  if (check.status === "unknown") {
    return (
      <p className="flex items-start gap-1.5 text-xs text-muted-foreground">
        <CircleQuestionMark className="mt-px h-3.5 w-3.5 shrink-0" />
        Hello Print can&apos;t check this printer for enlargement. Print a test
        sheet and measure the 50 mm square.
      </p>
    );
  }

  const factor = check.factor ?? 1;
  const percent = ((factor - 1) * 100).toFixed(1);
  const fiftyMm = (50 * factor).toFixed(1);

  return (
    <div className="space-y-2 rounded-md border border-amber-500/50 bg-amber-500/10 p-3 text-xs">
      <p className="flex items-start gap-1.5 font-medium text-amber-700 dark:text-amber-400">
        <TriangleAlert className="mt-px h-3.5 w-3.5 shrink-0" />
        These settings enlarge prints by {percent}%: a 50 mm shape prints at
        about {fiftyMm} mm.
      </p>
      <p>Turn off borderless enlargement in the printer settings:</p>
      <ul className="list-disc space-y-0.5 pl-5">
        <li>
          <span className="font-medium">Epson:</span> Borderless &rarr;
          Settings&hellip; &rarr; Retain Size
        </li>
        <li>
          <span className="font-medium">Canon:</span> Amount of Extension &rarr;
          slide all the way left
        </li>
        <li>
          <span className="font-medium">Other printers:</span> look for
          borderless &ldquo;expansion&rdquo; or &ldquo;enlargement&rdquo; and
          choose off or the smallest setting
        </li>
      </ul>
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={fixDisabled}
        onClick={onFix}
      >
        <Settings2 className="mr-2 h-4 w-4" />
        Fix in Printer Settings
      </Button>
    </div>
  );
}
