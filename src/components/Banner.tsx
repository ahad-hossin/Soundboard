import { useStore } from "../lib/store";

export function Banner() {
  const setPanel = useStore((s) => s.setPanel);
  return (
    <div className="mb-3 rounded-md border border-border bg-surface px-3 py-2 text-dim">
      No output device selected.{" "}
      <button type="button" className="text-text underline underline-offset-2" onClick={() => setPanel("settings")}>
        Open Settings
      </button>
    </div>
  );
}
