import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { listPrinters } from "@/lib/api";
import type { PrinterInfo } from "@/lib/types";

export function PrinterSelector({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (printerId: string) => void;
}) {
  const [printers, setPrinters] = useState<PrinterInfo[]>([]);
  const [loading, setLoading] = useState(false);

  const fetchPrinters = async () => {
    setLoading(true);

    try {
      const result = await listPrinters();
      setPrinters(result);
    } catch (err) {
      console.error("Failed to fetch printers:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPrinters();
  }, []);

  return (
    <div className="flex items-center gap-2">
      <Select value={value ?? ""} onValueChange={onChange}>
        <SelectTrigger className="flex-1">
          <SelectValue placeholder="Select a printer..." />
        </SelectTrigger>
        <SelectContent>
          {printers.map((p) => (
            <SelectItem key={p.id} value={p.id}>
              <span className="flex items-center gap-2">
                {p.name}
                {p.is_default && (
                  <Badge variant="secondary" className="text-xs">
                    Default
                  </Badge>
                )}
                {p.is_generic_driver && (
                  <Badge
                    variant="outline"
                    className="border-amber-500/50 text-xs text-amber-700 dark:text-amber-400"
                  >
                    Generic driver
                  </Badge>
                )}
              </span>
            </SelectItem>
          ))}
          {printers.length === 0 && !loading && (
            <div className="px-2 py-1.5 text-sm text-muted-foreground">
              No printers found
            </div>
          )}
        </SelectContent>
      </Select>
      <Button
        variant="outline"
        size="icon"
        onClick={fetchPrinters}
        disabled={loading}
      >
        <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
      </Button>
    </div>
  );
}
