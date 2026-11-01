import { useEffect, useLayoutEffect, useRef, useState } from "react";

/**
 * Shared behaviour of the floating panels (context menu, color picker): clamp the position into
 * the window and close on Esc, an outside click or window blur.
 */
export function usePopoverPosition(x: number, y: number, onClose: () => void) {
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
    // Capture phase + stopPropagation: Esc closes the panel without reaching the global Stop All.
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

  return { ref, pos };
}
