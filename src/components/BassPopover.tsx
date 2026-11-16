import { usePopoverPosition } from "../lib/usePopoverPosition";
import { useSliderDraft } from "../lib/useSliderDraft";

interface Props { x: number; y: number; name: string; value: number; onCommit: (v: number) => void; onClose: () => void }

/** Bass boost strength for one sound; saves when the slider is released. */
export function BassPopover({ x, y, name, value, onCommit, onClose }: Props) {
  const { ref, pos } = usePopoverPosition(x, y, onClose);
  const bass = useSliderDraft(value, onCommit);
  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Bass boost"
      style={{ left: pos.x, top: pos.y }}
      className="fixed z-50 flex w-60 items-center gap-2 rounded-md border border-border bg-surface p-3 shadow-xl"
    >
      <span className="text-[12px] text-dim">Bass</span>
      <input type="range" min={0} max={1} step={0.05} {...bass} aria-label={`${name} bass boost`} className="min-w-0 flex-1" />
      <span className="w-9 text-right font-mono text-[11px] tabular-nums text-dim">{Math.round(bass.value * 100)}%</span>
    </div>
  );
}
