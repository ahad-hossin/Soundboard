import { cx } from "../lib/cx";
import { PALETTE } from "../lib/palette";
import { usePopoverPosition } from "../lib/usePopoverPosition";

interface Props { x: number; y: number; current: string; onPick: (color: string) => void; onClose: () => void }

export function ColorPicker({ x, y, current, onPick, onClose }: Props) {
  const { ref, pos } = usePopoverPosition(x, y, onClose);

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Tile color"
      style={{ left: pos.x, top: pos.y }}
      className="fixed z-50 grid grid-cols-5 gap-2 rounded-md border border-border bg-surface p-2 shadow-xl"
    >
      {PALETTE.map((c) => {
        const selected = c.toLowerCase() === current.toLowerCase();
        return (
          <button
            key={c}
            type="button"
            aria-label={c}
            aria-pressed={selected}
            onClick={() => { onPick(c); onClose(); }}
            style={{ backgroundColor: c }}
            className={cx("h-6 w-6 rounded-md outline-none", selected ? "ring-2 ring-text" : "ring-accent focus-visible:ring-2")}
          />
        );
      })}
    </div>
  );
}
