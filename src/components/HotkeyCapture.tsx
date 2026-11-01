import { useEffect, useState } from "react";
import { accelFromEvent, displayAccel } from "../lib/accel";

interface Props {
  title: string;
  current: string | null;
  onCapture: (accelerator: string) => void;
  onClear?: () => void;
  onCancel: () => void;
}

/** Captures the next key combination and commits it immediately. Plain Esc cancels. */
export function HotkeyCapture({ title, current, onCapture, onClear, onCancel }: Props) {
  const [hint, setHint] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const bare = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
      if (e.code === "Escape" && bare) { onCancel(); return; }
      const accel = accelFromEvent(e);
      if (accel) { onCapture(accel); return; }
      if (e.code && !/^(Control|Shift|Alt|Meta)(Left|Right)$/.test(e.code)) {
        setHint("That key needs a modifier such as Ctrl or Alt. F-keys may be used alone.");
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCapture, onCancel]);

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50" onMouseDown={onCancel}>
      <div role="dialog" className="w-80 rounded-md border border-border bg-surface p-4 shadow-xl" onMouseDown={(e) => e.stopPropagation()}>
        <p className="mb-1 text-dim">{title}</p>
        <p className="mb-3 font-mono text-lg">{current ? displayAccel(current) : "Press keys…"}</p>
        <p className="mb-4 min-h-8 text-xs text-mute">{hint ?? "Press a key combination. Esc to cancel."}</p>
        <div className="flex justify-end gap-2">
          {onClear && <button type="button" className="btn border-danger text-danger" onClick={onClear}>Clear</button>}
          <button type="button" className="btn" onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
