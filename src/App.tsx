import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeProvider } from "@/components/theme-provider";
import { AppSidebar, type View } from "@/components/app-sidebar";
import { DashboardPage } from "@/pages/dashboard";
import { PresetsPage } from "@/pages/presets";
import { QueuePage } from "@/pages/queue";
import { SettingsPage } from "@/pages/settings";
import "./App.css";

const views: Record<View, React.ComponentType> = {
  dashboard: DashboardPage,
  presets: PresetsPage,
  queue: QueuePage,
  settings: SettingsPage,
};

function App() {
  const [activeView, setActiveView] = useState<View>("dashboard");
  const ActivePage = views[activeView];

  // The window starts hidden (tauri.conf.json) so the webview's blank white
  // background never shows. This runs after ThemeProvider has applied the
  // theme; reveal the window on the next frame, once it has painted.
  useEffect(() => {
    requestAnimationFrame(() => {
      getCurrentWindow().show().catch(console.error);
    });
  }, []);

  return (
    <ThemeProvider defaultTheme="system">
      <TooltipProvider>
        <div className="flex h-screen overflow-hidden">
          <AppSidebar activeView={activeView} onNavigate={setActiveView} />
          <main className="flex-1 overflow-y-auto p-6">
            <ActivePage />
          </main>
        </div>
      </TooltipProvider>
    </ThemeProvider>
  );
}

export default App;
