import { convertFileSrc } from "@tauri-apps/api/core";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { MouseEvent } from "react";
import { displayAccel } from "../lib/accel";
import type { UiSound } from "../lib/api";
import { cx } from "../lib/cx";

interface Props {
  sound: UiSound;
  playing: boolean;
  onPlay: () => void;
  onVolume: (v: number) => void;
  onContextMenu: (e: MouseEvent) => void;
}

export function Tile({ sound, playing, onPlay, onVolume, onContextMenu }: Props) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: sound.id });
  const style = { transform: CSS.Transform.toString(transform), transition };
  const src = sound.image_path ? convertFileSrc(sound.image_path) : null;

  return (
    <div ref={setNodeRef} style={style} className={cx("group relative aspect-square", isDragging && "z-10 opacity-80")}>
      <button
        type="button"
        {...attributes}
        {...listeners}
        onClick={onPlay}
        onContextMenu={onContextMenu}
        aria-label={sound.name}
        aria-pressed={playing}
        className={cx(
          "block h-full w-full overflow-hidden rounded-md bg-surface-2 text-left outline-none ring-accent transition-shadow focus-visible:ring-2",
          playing && "ring-2",
        )}
      >
        {src && <img src={src} alt="" draggable={false} className="pointer-events-none absolute inset-0 h-full w-full object-cover" />}
        <span
          className={cx(
            "absolute inset-x-0 bottom-0 truncate p-2 text-[13px] font-medium leading-tight",
            src && "bg-gradient-to-t from-black/70 to-transparent pt-6",
          )}
        >
          {sound.name}
        </span>
        {sound.hotkey && (
          <span className="absolute right-1.5 top-1.5 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[11px] text-dim">
            {displayAccel(sound.hotkey)}
          </span>
        )}
        {sound.missing_file && (
          <span title="File missing" className="absolute left-1.5 top-1.5 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[11px] text-danger">
            !
          </span>
        )}
      </button>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={sound.volume}
        aria-label={`${sound.name} volume`}
        onChange={(e) => onVolume(Number(e.target.value))}
        onPointerDown={(e) => e.stopPropagation()}
        className="absolute inset-x-2 bottom-7 opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
      />
    </div>
  );
}
