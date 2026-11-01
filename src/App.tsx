import { useEffect } from "react";
import { TileGrid } from "./components/TileGrid";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import { api } from "./lib/api";
import { isTypingTarget } from "./lib/keys";
import { useStore } from "./lib/store";

export default function App() {
  const init = useStore((s) => s.init);
  const state = useStore((s) => s.state);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isTypingTarget(e.target)) void api.stopAll();
    };
    const noMenu = (e: MouseEvent) => e.preventDefault();
    window.addEventListener("keydown", onKey);
    window.addEventListener("contextmenu", noMenu);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", noMenu);
    };
  }, []);

  if (!state) return <div className="h-full" />;

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 overflow-y-auto p-3">
          <TileGrid />
        </main>
      </div>
      <Toasts />
    </div>
  );
}
