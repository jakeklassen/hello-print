import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Check, Download } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { listPaperSizes, saveTestSheet } from "@/lib/api";
import type { PaperSize } from "@/lib/types";

// Saves the reference test sheet for a paper size as a PNG, to print from any
// app and check size, edges and centring.
export function TestSheetCard() {
  const [sizes, setSizes] = useState<PaperSize[]>([]);
  const [slug, setSlug] = useState("4x6");
  const [savedTo, setSavedTo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listPaperSizes()
      .then(setSizes)
      .catch((err) => setError(String(err)));
  }, []);

  const handleSave = async () => {
    setError(null);
    setSavedTo(null);

    const path = await save({
      defaultPath: `hello-print-test-sheet-${slug}.png`,
      filters: [{ name: "PNG image", extensions: ["png"] }],
    });

    if (!path) {
      return;
    }

    try {
      await saveTestSheet(slug, path);
      setSavedTo(path);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Test Sheets</CardTitle>
        <CardDescription>
          A printable sheet for checking prints come out true to size, centred
          and not cut off. Print it borderless at 100% and measure the 50 mm
          square.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid gap-1.5">
          <Label>Paper Size</Label>
          <div className="flex items-center gap-2">
            <Select
              value={slug}
              onValueChange={(v) => {
                setSlug(v);
                setSavedTo(null);
              }}
            >
              <SelectTrigger className="flex-1">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {sizes.map((s) => (
                  <SelectItem key={s.slug} value={s.slug}>
                    {s.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button variant="outline" onClick={handleSave}>
              <Download className="mr-2 h-4 w-4" />
              Save Test Sheet…
            </Button>
          </div>
        </div>
        {savedTo && (
          <p className="flex items-center gap-1 text-sm text-green-600">
            <Check className="h-4 w-4" />
            Saved to {savedTo}
          </p>
        )}
        {error && <p className="text-sm text-destructive">{error}</p>}
      </CardContent>
    </Card>
  );
}
