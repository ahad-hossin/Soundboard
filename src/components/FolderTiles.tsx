import type { MouseEvent } from "react";
import type { UiFolder } from "../lib/api";
import { cx } from "../lib/cx";
import type { FolderSummary } from "../lib/view";

const TILE =
  "relative flex aspect-square w-full flex-col rounded-md border border-border outline-none ring-accent transition-[box-shadow,transform] duration-100 hover:border-dim focus-visible:ring-2 active:scale-[0.96]";

interface FolderProps { folder: UiFolder; summary: FolderSummary; onOpen: () => void; onContextMenu: (e: MouseEvent) => void }

/** Monochrome on purpose: only sound tiles carry color. The 2×2 preview shows the first four sounds' colors. */
export function FolderTile({ folder, summary, onOpen, onContextMenu }: FolderProps) {
  return (
    <button
      type="button"
      onClick={onOpen}
      onContextMenu={onContextMenu}
      aria-label={`Folder ${folder.name}, ${summary.count} ${summary.count === 1 ? "sound" : "sounds"}`}
      className={cx(TILE, "bg-surface p-2 text-left", summary.playing && "ring-2")}
    >
      <span aria-hidden className="grid min-h-0 flex-1 grid-cols-2 grid-rows-2 gap-1">
        {[0, 1, 2, 3].map((i) => (
          <span key={i} className="rounded-sm bg-surface-2" style={summary.colors[i] ? { backgroundColor: summary.colors[i] } : undefined} />
        ))}
      </span>
      <span className="mt-2 flex items-baseline gap-1.5">
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{folder.name}</span>
        <span className="font-mono text-[11px] tabular-nums text-dim">{summary.count}</span>
      </span>
    </button>
  );
}

export function BackTile({ onBack }: { onBack: () => void }) {
  return (
    <button type="button" onClick={onBack} aria-label="Back to top level" className={cx(TILE, "items-center justify-center gap-1 text-dim hover:text-text")}>
      <span aria-hidden className="text-2xl leading-none">←</span>
      <span className="text-[13px]">Back</span>
    </button>
  );
}

export function NewFolderTile({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label="New folder"
      title="New folder"
      className="flex aspect-square flex-col items-center justify-center gap-1 rounded-md border border-dashed border-border text-mute outline-none ring-accent transition-colors hover:border-dim hover:text-dim focus-visible:ring-2"
    >
      <span aria-hidden className="text-2xl font-light leading-none">+</span>
      <span className="text-[12px]">Folder</span>
    </button>
  );
}
