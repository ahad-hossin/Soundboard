import { convertFileSrc } from "@tauri-apps/api/core";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { MouseEvent } from "react";
import { displayAccel } from "../lib/accel";
import type { UiSound } from "../lib/api";
import { cx } from "../lib/cx";
import { tileDecoration } from "../lib/decor";
import { useSliderDraft } from "../lib/useSliderDraft";

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
  const src = sound.image_path ? `${convertFileSrc(sound.image_path)}?v=${sound.image_rev}` : null;
  const volume = useSliderDraft(sound.volume, onVolume);

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
        style={src ? undefined : tileDecoration(sound.id, sound.color)}
        className={cx(
          "relative block h-full w-full overflow-hidden rounded-md bg-surface-2 text-left text-white outline-none ring-accent transition-[box-shadow,transform] duration-100 focus-visible:ring-2 active:scale-[0.96]",
          playing && "ring-2",
        )}
      >
        {src && (
          <img src={src} alt="" draggable={false} className="pointer-events-none absolute inset-0 h-full w-full object-cover" />
        )}
        <span className="absolute inset-x-0 bottom-0 truncate bg-gradient-to-t from-black/70 to-transparent p-2 pt-6 text-[13px] font-medium leading-tight">
          {sound.name}
        </span>
        {src && <span aria-hidden className="absolute inset-x-0 bottom-0 h-[3px]" style={{ backgroundColor: sound.color }} />}
        {playing && (
          <span
            key={sound.play_seq}
            data-testid="sweep"
            data-seq={sound.play_seq}
            aria-hidden
            className={cx("pointer-events-none absolute bottom-0 left-0 h-0.5 bg-white/85", sound.duration_secs == null ? "sweep-pulse" : "sweep-bar")}
            style={sound.duration_secs == null ? undefined : { animationDuration: `${sound.duration_secs}s` }}
          />
        )}
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
        tabIndex={-1}
        {...volume}
        aria-label={`${sound.name} volume`}
        onPointerDown={(e) => e.stopPropagation()}
        className="absolute inset-x-2 bottom-7 opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
      />
    </div>
  );
}
