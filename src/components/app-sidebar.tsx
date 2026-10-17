import {
  LayoutDashboard,
  ListChecks,
  Printer,
  Settings,
  Sun,
  Moon,
  Monitor,
} from "lucide-react";
import appIcon from "@/assets/app-icon.png";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Separator } from "@/components/ui/separator";
import { useTheme } from "@/components/theme-provider";
import { cn } from "@/lib/utils";

export type View = "dashboard" | "presets" | "queue" | "settings";

const navItems: { id: View; label: string; icon: React.ElementType }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "presets", label: "Presets", icon: Printer },
  { id: "queue", label: "Queue", icon: ListChecks },
  { id: "settings", label: "Settings", icon: Settings },
];

export function AppSidebar({
  activeView,
  onNavigate,
}: {
  activeView: View;
  onNavigate: (view: View) => void;
}) {
  const { theme, setTheme } = useTheme();

  const cycleTheme = () => {
    const order: Array<"light" | "dark" | "system"> = [
      "light",
      "dark",
      "system",
    ];
    const next =
      order[
        (order.indexOf(theme as "light" | "dark" | "system") + 1) % order.length
      ];
    setTheme(next);
  };

  const ThemeIcon = theme === "dark" ? Moon : theme === "light" ? Sun : Monitor;

  return (
    <aside className="flex h-full w-14 flex-col items-center border-r bg-card py-3">
      <img
        src={appIcon}
        alt="Hello Print"
        className="mb-2 h-8 w-8 rounded-md"
        draggable={false}
      />
      <Separator className="mb-2 w-8" />
      <nav className="flex flex-1 flex-col items-center gap-1">
        {navItems.map(({ id, label, icon: Icon }) => (
          <Tooltip key={id} delayDuration={0}>
            <TooltipTrigger asChild>
              <Button
                variant={activeView === id ? "secondary" : "ghost"}
                size="icon"
                className={cn("h-9 w-9", activeView === id && "bg-secondary")}
                onClick={() => onNavigate(id)}
              >
                <Icon className="h-4 w-4" />
                <span className="sr-only">{label}</span>
              </Button>
            </TooltipTrigger>
            <TooltipContent side="right">{label}</TooltipContent>
          </Tooltip>
        ))}
      </nav>
      <Tooltip delayDuration={0}>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-9 w-9"
            onClick={cycleTheme}
          >
            <ThemeIcon className="h-4 w-4" />
            <span className="sr-only">Toggle theme ({theme})</span>
          </Button>
        </TooltipTrigger>
        <TooltipContent side="right">Theme: {theme}</TooltipContent>
      </Tooltip>
    </aside>
  );
}
