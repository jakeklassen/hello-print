import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  FolderOpen,
  Save,
  RefreshCw,
  Download,
  Check,
  FileText,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import {
  getConfig,
  saveConfig,
  startWatcher,
  getPlatform,
  openLogFolder,
} from "@/lib/api";
import { getVersion } from "@tauri-apps/api/app";
import type { AppConfig, PostFileAction } from "@/lib/types";

type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "up-to-date"
  | "error";

export function SettingsPage() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [dirty, setDirty] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const [updateState, setUpdateState] = useState<UpdateState>("idle");
  const [updateVersion, setUpdateVersion] = useState("");
  const [updateProgress, setUpdateProgress] = useState("");
  const [updateRef, setUpdateRef] = useState<Update | null>(null);
  const [platform, setPlatform] = useState("");

  useEffect(() => {
    getVersion().then(setAppVersion);
    getPlatform().then(setPlatform);
  }, []);

  const handleCheckUpdate = async () => {
    setUpdateState("checking");
    setUpdateProgress("");

    try {
      const update = await check();

      if (update) {
        setUpdateRef(update);
        setUpdateVersion(update.version);
        setUpdateState("available");
      } else {
        setUpdateState("up-to-date");
      }
    } catch (e) {
      // No releases yet or network error — treat as up-to-date
      console.debug("Update check:", e);
      setUpdateState("up-to-date");
    }
  };

  const handleInstallUpdate = async () => {
    if (!updateRef) {
      return;
    }

    setUpdateState("downloading");

    try {
      let totalLength = 0;
      let downloaded = 0;
      await updateRef.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            totalLength = event.data.contentLength ?? 0;
            setUpdateProgress("Downloading...");
            break;
          case "Progress":
            downloaded += event.data.chunkLength;

            if (totalLength > 0) {
              const pct = Math.round((downloaded / totalLength) * 100);
              setUpdateProgress(`Downloading... ${pct}%`);
            }

            break;
          case "Finished":
            setUpdateProgress("Restart the app to apply the update");
            break;
        }
      });
      setUpdateState("ready");
    } catch (e) {
      console.error("Update install failed:", e);
      setUpdateState("error");
      setUpdateProgress(e instanceof Error ? e.message : "Install failed");
    }
  };

  useEffect(() => {
    getConfig().then(setConfig);
  }, []);

  const update = (partial: Partial<AppConfig>) => {
    if (!config) {
      return;
    }

    setConfig({ ...config, ...partial });
    setDirty(true);
  };

  const handleSave = async () => {
    if (!config) {
      return;
    }

    await saveConfig(config);
    setDirty(false);
  };

  const handleSelectFolder = async () => {
    const folder = await open({ directory: true });

    if (folder && config) {
      const newConfig: AppConfig = { ...config, watch_folder: folder };
      await saveConfig(newConfig);
      await startWatcher(folder);
      setConfig(newConfig);
      setDirty(false);
    }
  };

  if (!config) {
    return null;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        {dirty && (
          <Button onClick={handleSave}>
            <Save className="mr-2 h-4 w-4" />
            Save Changes
          </Button>
        )}
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Watch Folder</CardTitle>
          <CardDescription>
            The folder Hello Print monitors for new files.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-2">
            <code className="flex-1 rounded bg-muted px-3 py-2 text-sm">
              {config.watch_folder || "Not configured"}
            </code>
            <Button variant="outline" onClick={handleSelectFolder}>
              <FolderOpen className="mr-2 h-4 w-4" />
              Browse
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Behavior</CardTitle>
          <CardDescription>
            How the app behaves when minimized or closed.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <Label>Minimize to System Tray</Label>
              <p className="text-xs text-muted-foreground">
                Keep watching and printing in the background when the window is
                closed.
              </p>
            </div>
            <Switch
              checked={config.minimize_to_tray}
              onCheckedChange={(v) => update({ minimize_to_tray: v })}
            />
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">File Handling</CardTitle>
          <CardDescription>
            What happens to files after processing.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-1.5">
            <Label>After Printing an Image</Label>
            <Select
              value={config.post_print_action}
              onValueChange={(v: PostFileAction) =>
                update({ post_print_action: v })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="move_to_subfolder">
                  Move to "printed" subfolder
                </SelectItem>
                <SelectItem value="delete">Delete</SelectItem>
                <SelectItem value="keep">Keep in place</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <Separator />

          <div className="grid gap-1.5">
            <Label>After Extracting a Zip</Label>
            <Select
              value={config.post_zip_action}
              onValueChange={(v: PostFileAction) =>
                update({ post_zip_action: v })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="delete">Delete the zip</SelectItem>
                <SelectItem value="move_to_subfolder">
                  Move to "processed_zips" subfolder
                </SelectItem>
                <SelectItem value="keep">Keep in place</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle className="text-base">About</CardTitle>
          <CardDescription>
            Hello Print v{appVersion}
            {platform ? ` (${platform})` : ""} — Made by Jake Klassen of{" "}
            <a
              href="https://hellomagnets.studio"
              target="_blank"
              rel="noopener noreferrer"
              className="underline hover:text-foreground"
            >
              Hello Magnets
            </a>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-2">
            {updateState === "idle" && (
              <Button variant="outline" onClick={handleCheckUpdate}>
                <RefreshCw className="mr-2 h-4 w-4" />
                Check for Updates
              </Button>
            )}
            {updateState === "checking" && (
              <Button variant="outline" disabled>
                <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                Checking...
              </Button>
            )}
            {updateState === "up-to-date" && (
              <div className="flex items-center gap-2">
                <span className="flex items-center gap-1 text-sm text-green-600">
                  <Check className="h-4 w-4" />
                  You're on the latest version
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setUpdateState("idle")}
                >
                  Check again
                </Button>
              </div>
            )}
            {updateState === "available" && (
              <div className="flex items-center gap-2">
                <span className="text-sm">v{updateVersion} available</span>
                <Button variant="outline" onClick={handleInstallUpdate}>
                  <Download className="mr-2 h-4 w-4" />
                  Install Update
                </Button>
              </div>
            )}
            {updateState === "downloading" && (
              <span className="text-sm">{updateProgress}</span>
            )}
            {updateState === "ready" && (
              <span className="flex items-center gap-1 text-sm text-green-600">
                <Check className="h-4 w-4" />
                {updateProgress}
              </span>
            )}
            {updateState === "error" && (
              <div className="flex items-center gap-2">
                <span className="text-sm text-destructive">
                  {updateProgress}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setUpdateState("idle")}
                >
                  Retry
                </Button>
              </div>
            )}
          </div>
          <Separator />
          <div className="flex items-center justify-between gap-4">
            <p className="text-sm text-muted-foreground">
              Having trouble? Send us the log files from this folder.
            </p>
            <Button
              variant="outline"
              onClick={() => openLogFolder().catch(console.error)}
            >
              <FileText className="mr-2 h-4 w-4" />
              Open Logs Folder
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
