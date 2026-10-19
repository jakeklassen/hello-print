import { useEffect, useState } from "react";
import { Plus, Pencil, Trash2, Copy, Star, MoreVertical } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PresetForm } from "@/components/preset-form";
import {
  listPresets,
  createPreset,
  updatePreset,
  deletePreset,
  getConfig,
  saveConfig,
  listPrinters,
} from "@/lib/api";
import type { PresetFormData } from "@/components/preset-form";
import type { AppConfig, Preset, PrinterInfo } from "@/lib/types";

export function PresetsPage() {
  const [presets, setPresets] = useState<Preset[]>([]);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [printers, setPrinters] = useState<PrinterInfo[]>([]);
  const [editingPreset, setEditingPreset] = useState<Preset | null>(null);
  const [showCreate, setShowCreate] = useState(false);

  const refresh = async () => {
    const [p, c] = await Promise.all([listPresets(), getConfig()]);
    setPresets(p);
    setConfig(c);
    // Only used for the driver badges; the list still works without them.
    listPrinters()
      .then(setPrinters)
      .catch(() => setPrinters([]));
  };

  useEffect(() => {
    refresh();
  }, []);

  const handleCreate = async (data: PresetFormData) => {
    const preset = await createPreset(data.name, data.paper_size_keyword);
    const updated: Preset = {
      ...preset,
      printer_id: data.printer_id,
      settings: data.settings,
      copies: data.copies,
      auto_print: data.auto_print,
      scale_compensation: data.scale_compensation,
      devmode_base64: data.devmode_base64,
      macos_print_info_base64: data.macos_print_info_base64,
      macos_page_format_base64: data.macos_page_format_base64,
      macos_print_settings_base64: data.macos_print_settings_base64,
      macos_printer_name: data.macos_printer_name,
      macos_page_width_points: data.macos_page_width_points,
      macos_page_height_points: data.macos_page_height_points,
      macos_size_compensation_mm: data.macos_size_compensation_mm,
    };
    await updatePreset(updated);
    setShowCreate(false);
    refresh();
  };

  const handleUpdate = async (data: PresetFormData) => {
    if (!editingPreset) {
      return;
    }

    const updated: Preset = {
      ...editingPreset,
      name: data.name,
      printer_id: data.printer_id,
      paper_size_keyword: data.paper_size_keyword,
      settings: data.settings,
      copies: data.copies,
      auto_print: data.auto_print,
      scale_compensation: data.scale_compensation,
      devmode_base64: data.devmode_base64,
      macos_print_info_base64: data.macos_print_info_base64,
      macos_page_format_base64: data.macos_page_format_base64,
      macos_print_settings_base64: data.macos_print_settings_base64,
      macos_printer_name: data.macos_printer_name,
      macos_page_width_points: data.macos_page_width_points,
      macos_page_height_points: data.macos_page_height_points,
      macos_size_compensation_mm: data.macos_size_compensation_mm,
      updated_at: new Date().toISOString(),
    };
    await updatePreset(updated);
    setEditingPreset(null);
    refresh();
  };

  const handleDuplicate = async (preset: Preset) => {
    const newPreset = await createPreset(
      `${preset.name} (Copy)`,
      preset.paper_size_keyword,
    );
    const duplicated: Preset = {
      ...newPreset,
      printer_id: preset.printer_id,
      settings: { ...preset.settings },
      copies: preset.copies,
      auto_print: preset.auto_print,
      scale_compensation: preset.scale_compensation,
      devmode_base64: preset.devmode_base64,
      macos_print_info_base64: preset.macos_print_info_base64,
      macos_page_format_base64: preset.macos_page_format_base64,
      macos_print_settings_base64: preset.macos_print_settings_base64,
      macos_printer_name: preset.macos_printer_name,
      macos_page_width_points: preset.macos_page_width_points,
      macos_page_height_points: preset.macos_page_height_points,
      macos_size_compensation_mm: preset.macos_size_compensation_mm,
    };
    await updatePreset(duplicated);
    refresh();
  };

  const handleDelete = async (id: string) => {
    await deletePreset(id);
    refresh();
  };

  const handleSetDefault = async (id: string) => {
    if (!config) {
      return;
    }

    const newConfig: AppConfig = {
      ...config,
      default_preset_id: config.default_preset_id === id ? null : id,
    };
    await saveConfig(newConfig);
    refresh();
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Presets</h1>
        <Button onClick={() => setShowCreate(true)}>
          <Plus className="mr-2 h-4 w-4" />
          New Preset
        </Button>
      </div>

      {presets.length === 0 ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">
            No presets yet. Create one to get started.
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3">
          {presets.map((preset) => {
            const isDefault = config?.default_preset_id === preset.id;
            const printer = printers.find((p) => p.id === preset.printer_id);
            return (
              <Card key={preset.id}>
                <CardHeader className="flex flex-row items-center justify-between py-3 px-4">
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="min-w-0">
                      <CardTitle className="text-base flex items-center gap-2">
                        {preset.name}
                        {isDefault && (
                          <Badge variant="secondary" className="text-xs">
                            <Star className="mr-1 h-3 w-3" />
                            Default
                          </Badge>
                        )}
                        <DriverBadge printer={printer} />
                      </CardTitle>
                      <CardDescription className="text-xs mt-0.5">
                        {preset.printer_id ?? "No printer"} &middot; Keyword:{" "}
                        <code>{preset.paper_size_keyword}</code> &middot;{" "}
                        {preset.copies}{" "}
                        {preset.copies === 1 ? "copy" : "copies"} &middot;{" "}
                        {preset.auto_print ? "Auto-print" : "Confirm first"}
                      </CardDescription>
                    </div>
                  </div>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button variant="ghost" size="icon" className="h-8 w-8">
                        <MoreVertical className="h-4 w-4" />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuItem
                        onClick={() => setEditingPreset(preset)}
                      >
                        <Pencil className="mr-2 h-4 w-4" />
                        Edit
                      </DropdownMenuItem>
                      <DropdownMenuItem onClick={() => handleDuplicate(preset)}>
                        <Copy className="mr-2 h-4 w-4" />
                        Duplicate
                      </DropdownMenuItem>
                      <DropdownMenuItem
                        onClick={() => handleSetDefault(preset.id)}
                      >
                        <Star className="mr-2 h-4 w-4" />
                        {isDefault ? "Unset Default" : "Set as Default"}
                      </DropdownMenuItem>
                      <DropdownMenuItem
                        className="text-destructive"
                        onClick={() => handleDelete(preset.id)}
                      >
                        <Trash2 className="mr-2 h-4 w-4" />
                        Delete
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </CardHeader>
              </Card>
            );
          })}
        </div>
      )}

      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent className="max-w-lg max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Create Preset</DialogTitle>
          </DialogHeader>
          <PresetForm
            onSave={handleCreate}
            onCancel={() => setShowCreate(false)}
          />
        </DialogContent>
      </Dialog>

      <Dialog
        open={!!editingPreset}
        onOpenChange={(open) => !open && setEditingPreset(null)}
      >
        <DialogContent className="max-w-lg max-h-[85vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Edit Preset</DialogTitle>
          </DialogHeader>
          {editingPreset && (
            <PresetForm
              preset={editingPreset}
              onSave={handleUpdate}
              onCancel={() => setEditingPreset(null)}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

// Windows only (macOS doesn't report drivers): whether the preset's printer
// uses the manufacturer's driver or Windows' generic one, which can't stop
// borderless prints being enlarged.
function DriverBadge({ printer }: { printer: PrinterInfo | undefined }) {
  if (!printer?.driver) {
    return null;
  }

  if (printer.is_generic_driver) {
    return (
      <Badge
        variant="outline"
        className="border-amber-500/50 text-xs text-amber-700 dark:text-amber-400"
        title={printer.driver}
      >
        Generic driver
      </Badge>
    );
  }

  return (
    <Badge
      variant="outline"
      className="border-green-600/50 text-xs text-green-700 dark:text-green-400"
      title={printer.driver}
    >
      Manufacturer driver
    </Badge>
  );
}
