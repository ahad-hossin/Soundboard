import type { MouseEvent } from "react";

interface Props { count: number; onMove?: (e: MouseEvent<HTMLButtonElement>) => void; onClear: () => void }

export function SelectionBar({ count, onMove, onClear }: Props) {
  return (
    <div role="group" aria-label="Selection" className="mb-2 flex h-9 items-center gap-2 rounded-md border border-border bg-surface px-3">
      <span className="tabular-nums">{count} selected</span>
      <div className="flex-1" />
      {onMove && <button type="button" className="btn" onClick={onMove}>Move to folder…</button>}
      <button type="button" className="btn" onClick={onClear}>Clear</button>
    </div>
  );
}
