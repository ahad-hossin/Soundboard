import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { api, type UiSound, type Waveform } from "../lib/api";
import { useStore } from "../lib/store";
import { formatTime, parseTime } from "../lib/time";
import { clampRange, nudge, pxToSecs, secsToPx, shiftRange, type Handle, type Range } from "../lib/trimMath";
import { Backdrop } from "./Dialogs";

const CANVAS_H = 100;
/** Inner width of the 560 px dialog (16 px padding each side); used until the box is measured. */
const FALLBACK_W = 528;
/** A selection this close to both ends counts as the whole file, so Save clears the trim. */
const WHOLE_TOLERANCE = 0.01;
/** Pixels of handle grab area on each side of a handle line. */
const GRAB = 5;
/** Smallest gap between ruler labels, in px. */
const LABEL_GAP = 36;
const LABEL_STEPS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600];

type Drag = { kind: Handle } | { kind: "move"; anchor: number; from: Range };

interface Props { sound: UiSound; mode: "edit" | "import"; onClose: () => void }

/** A color token from index.css, falling back to its exact hex when styles are not loaded. */
function token(name: string, fallback: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

const roundMs = (secs: number) => Math.round(secs * 1000) / 1000;

function draw(canvas: HTMLCanvasElement, width: number, peaks: number[], range: Range, duration: number) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(CANVAS_H * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.globalAlpha = 1;
  ctx.fillStyle = token("--color-surface-2", "#1c1c1f");
  ctx.fillRect(0, 0, width, CANVAS_H);

  ctx.fillStyle = token("--color-dim", "#8a8a93");
  const barW = width / Math.max(peaks.length, 1);
  peaks.forEach((p, i) => {
    const h = Math.max(1, p * (CANVAS_H - 8));
    ctx.fillRect(i * barW, (CANVAS_H - h) / 2, Math.max(barW - 0.4, 0.5), h);
  });

  const accent = token("--color-accent", "#f5b324");
  const x0 = secsToPx(range.start, width, duration);
  const x1 = secsToPx(range.end, width, duration);
  ctx.fillStyle = accent;
  ctx.globalAlpha = 0.18;
  ctx.fillRect(x0, 0, x1 - x0, CANVAS_H);
  ctx.globalAlpha = 1;
  const line = (x: number) => ctx.fillRect(Math.min(Math.max(x - 1, 0), width - 2), 0, 2, CANVAS_H);
  line(x0);
  line(x1);
}

/** Second marks under the canvas; labels thin out so they never crowd. */
function Ruler({ width, duration }: { width: number; duration: number }) {
  const pxPerSec = duration > 0 ? width / duration : 0;
  const labelEvery = LABEL_STEPS.find((s) => s * pxPerSec >= LABEL_GAP) ?? LABEL_STEPS[LABEL_STEPS.length - 1];
  const tickEvery = pxPerSec >= 4 ? 1 : labelEvery;
  const marks: number[] = [];
  for (let t = 0; t <= duration; t += tickEvery) marks.push(t);
  return (
    <div aria-hidden className="relative h-4 overflow-hidden">
      {marks.map((t) => {
        const left = secsToPx(t, width, duration);
        return (
          <span key={t} className="absolute top-0" style={{ left }}>
            <span className="block h-1 w-px bg-mute" />
            {t % labelEvery === 0 && (
              <span className="absolute left-0.5 top-0.5 font-mono text-[11px] leading-none text-mute">
                {Math.floor(t / 60)}:{String(t % 60).padStart(2, "0")}
              </span>
            )}
          </span>
        );
      })}
    </div>
  );
}

function TimeField({ id, label, value, onCommit }: { id: string; label: string; value: number; onCommit: (secs: number) => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft !== null) {
      const v = parseTime(draft);
      if (v !== null) onCommit(v);
    }
    setDraft(null);
  };
  return (
    <div>
      <label className="label" htmlFor={id}>{label}</label>
      <input
        id={id}
        className="field w-28 font-mono"
        value={draft ?? formatTime(value)}
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }}
      />
    </div>
  );
}

/**
 * Non-destructive crop of one sound. Opened from the tile menu ("edit") or right after importing a
 * single file ("import", where Cancel reads Skip). Save stores the range, or clears it when the
 * whole file is selected.
 */
export function TrimEditor({ sound, mode, onClose }: Props) {
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [wave, setWave] = useState<Waveform | null>(null);
  const [range, setRange] = useState<Range>({ start: 0, end: 0 });
  const [width, setWidth] = useState(FALLBACK_W);
  const [previewing, setPreviewing] = useState(false);
  const boxRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drag = useRef<Drag | null>(null);
  const previewTimer = useRef<number | undefined>(undefined);
  const previewingRef = useRef(false);
  const duration = wave?.duration_secs ?? 0;

  useEffect(() => {
    let alive = true;
    api.getWaveform(sound.id).then(
      (w) => {
        if (!alive) return;
        setWave(w);
        setRange(w.trim ? clampRange(w.trim.start_secs, w.trim.end_secs, w.duration_secs) : { start: 0, end: w.duration_secs });
      },
      (e) => {
        if (!alive) return;
        toast("error", String(e));
        onClose();
      },
    );
    return () => { alive = false; };
    // Loads once per sound; a new onClose identity must not refetch.
  }, [sound.id]);

  useLayoutEffect(() => {
    const w = boxRef.current?.clientWidth;
    if (w) setWidth(w);
  }, [wave]);

  useEffect(() => {
    if (wave && canvasRef.current) draw(canvasRef.current, width, wave.peaks, range, wave.duration_secs);
  }, [wave, range, width]);

  /** Forgets the local preview state; returns whether a preview was running. */
  const endPreview = () => {
    window.clearTimeout(previewTimer.current);
    const was = previewingRef.current;
    previewingRef.current = false;
    setPreviewing(false);
    return was;
  };

  // Closing the editor any other way still silences a running preview.
  useEffect(() => () => {
    window.clearTimeout(previewTimer.current);
    if (previewingRef.current) void api.stopPreview();
  }, []);

  const togglePreview = async () => {
    if (previewingRef.current) {
      endPreview();
      void run(api.stopPreview());
      return;
    }
    try {
      await api.previewTrim(sound.id, range.start, range.end);
    } catch (e) {
      toast("error", String(e));
      return;
    }
    previewingRef.current = true;
    setPreviewing(true);
    window.clearTimeout(previewTimer.current);
    previewTimer.current = window.setTimeout(endPreview, (range.end - range.start) * 1000 + 100);
  };

  const escape = () => {
    endPreview();
    void run(api.stopPreview());
    onClose();
  };

  const cancel = () => {
    if (endPreview()) void run(api.stopPreview());
    onClose();
  };

  const save = () => {
    if (!wave) return;
    if (endPreview()) void run(api.stopPreview());
    const whole = range.start <= WHOLE_TOLERANCE && range.end >= wave.duration_secs - WHOLE_TOLERANCE;
    void run(api.setTrim(sound.id, whole ? null : { start_secs: roundMs(range.start), end_secs: roundMs(range.end) }));
    onClose();
  };

  const secsAt = (clientX: number) => {
    const rect = boxRef.current?.getBoundingClientRect();
    return pxToSecs(clientX - (rect?.left ?? 0), width, duration);
  };

  const onCanvasDown = (e: PointerEvent<HTMLCanvasElement>) => {
    if (!wave) return;
    const t = secsAt(e.clientX);
    if (t > range.start && t < range.end) {
      drag.current = { kind: "move", anchor: t, from: range };
    } else {
      // Outside the selection: the nearer handle jumps here and keeps following the pointer.
      const kind: Handle = Math.abs(t - range.start) < Math.abs(t - range.end) ? "start" : "end";
      drag.current = { kind };
      setRange((r) => (kind === "start" ? clampRange(t, r.end, duration, "start") : clampRange(r.start, t, duration, "end")));
    }
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };

  const onHandleDown = (kind: Handle) => (e: PointerEvent<HTMLDivElement>) => {
    e.stopPropagation();
    e.preventDefault();
    e.currentTarget.focus();
    drag.current = { kind };
    e.currentTarget.setPointerCapture?.(e.pointerId);
  };

  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const t = secsAt(e.clientX);
    if (d.kind === "move") setRange(shiftRange(d.from, t - d.anchor, duration));
    else setRange((r) => (d.kind === "start" ? clampRange(t, r.end, duration, "start") : clampRange(r.start, t, duration, "end")));
  };

  const onUp = () => { drag.current = null; };

  const onHandleKey = (kind: Handle) => (e: KeyboardEvent<HTMLDivElement>) => {
    const dir = e.key === "ArrowLeft" || e.key === "ArrowDown" ? -1 : e.key === "ArrowRight" || e.key === "ArrowUp" ? 1 : 0;
    if (!dir) return;
    e.preventDefault();
    setRange((r) => nudge(r, kind, dir * (e.shiftKey ? 0.1 : 0.01), duration));
  };

  const handle = (kind: Handle) => {
    const secs = kind === "start" ? range.start : range.end;
    return (
      <div
        role="slider"
        tabIndex={0}
        aria-label={kind === "start" ? "Trim start" : "Trim end"}
        aria-valuemin={0}
        aria-valuemax={roundMs(duration)}
        aria-valuenow={roundMs(secs)}
        aria-valuetext={formatTime(secs)}
        onPointerDown={onHandleDown(kind)}
        onKeyDown={onHandleKey(kind)}
        className="absolute top-0 cursor-ew-resize rounded-sm"
        style={{ left: secsToPx(secs, width, duration) - GRAB, width: GRAB * 2, height: CANVAS_H }}
      />
    );
  };

  return (
    <Backdrop onCancel={escape} className="w-[560px] max-w-[calc(100vw-32px)]">
      <p className="mb-3 truncate text-[13px] text-text">{sound.name}</p>
      {!wave ? (
        <p className="mb-4 text-dim">Loading…</p>
      ) : (
        <>
          <div ref={boxRef} className="relative touch-none" onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}>
            <canvas
              ref={canvasRef}
              aria-label={`Waveform of ${sound.name}`}
              className="block w-full cursor-pointer rounded-sm"
              style={{ height: CANVAS_H }}
              onPointerDown={onCanvasDown}
            />
            {handle("start")}
            {handle("end")}
          </div>
          <Ruler width={width} duration={duration} />
          <div className="mt-3 flex items-end gap-3">
            <TimeField id="trim-start" label="Start" value={range.start} onCommit={(v) => setRange((r) => clampRange(v, r.end, duration, "start"))} />
            <TimeField id="trim-end" label="End" value={range.end} onCommit={(v) => setRange((r) => clampRange(r.start, v, duration, "end"))} />
            <span aria-label="Selection length" className="pb-2 font-mono text-dim">{formatTime(range.end - range.start)}</span>
          </div>
        </>
      )}
      <div className="mt-4 flex items-center gap-2">
        <button type="button" className="btn" disabled={!wave} aria-pressed={previewing} onClick={() => void togglePreview()}>
          {previewing ? "Stop" : "Play selection"}
        </button>
        <button type="button" className="btn" disabled={!wave} onClick={() => setRange({ start: 0, end: duration })}>Reset</button>
        <span className="flex-1" />
        <button type="button" className="btn" onClick={cancel}>{mode === "import" ? "Skip" : "Cancel"}</button>
        <button type="button" className="btn border-text bg-text text-bg hover:bg-text" disabled={!wave} onClick={save}>Save</button>
      </div>
    </Backdrop>
  );
}
