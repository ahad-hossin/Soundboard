import { cx } from "../lib/cx";
import { usePopoverPosition } from "../lib/usePopoverPosition";

export interface MenuItem { label: string; onSelect: () => void; danger?: boolean }

interface Props { x: number; y: number; items: MenuItem[]; onClose: () => void }

export function ContextMenu({ x, y, items, onClose }: Props) {
  const { ref, pos } = usePopoverPosition(x, y, onClose);

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
