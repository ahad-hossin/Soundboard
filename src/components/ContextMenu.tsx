import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { cx } from "../lib/cx";

export interface MenuItem { label: string; onSelect: () => void; danger?: boolean }

interface Props { x: number; y: number; items: MenuItem[]; onClose: () => void }

export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.min(x, window.innerWidth - r.width - 8),
      y: Math.min(y, window.innerHeight - r.height - 8),
    });
  }, [x, y]);

  useEffect(() => {
    // Capture phase + stopPropagation: Esc closes the menu without reaching the global Stop All.
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); onClose(); } };
    const onDown = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) onClose(); };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return (
    <div ref={ref} role="menu" style={{ left: pos.x, top: pos.y }} className="fixed z-50 min-w-40 rounded-md border border-border bg-surface py-1 shadow-xl">
      {items.map((it) => (
        <button
          key={it.label}
          type="button"
          role="menuitem"
          onClick={() => { it.onSelect(); onClose(); }}
          className={cx("block w-full px-3 py-1.5 text-left hover:bg-surface-2", it.danger && "text-danger")}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}
