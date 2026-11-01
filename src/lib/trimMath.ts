/** Shortest selection the editor allows, in seconds. */
export const MIN_GAP = 0.05;

export interface Range { start: number; end: number }
export type Handle = "start" | "end";

/**
 * Keeps 0 <= start < end <= duration with at least `MIN_GAP` between them (the whole clip when it
 * is shorter). When the gap is violated, the `moving` handle is the one pushed back.
 */
export function clampRange(start: number, end: number, duration: number, moving: Handle = "end"): Range {
  const gap = Math.min(MIN_GAP, duration);
  if (moving === "start") {
    const e = Math.min(Math.max(end, gap), duration);
    return { start: Math.min(Math.max(start, 0), e - gap), end: e };
  }
  const s = Math.min(Math.max(start, 0), duration - gap);
  return { start: s, end: Math.max(Math.min(end, duration), s + gap) };
}

/** Seconds at `px` across a canvas `width` px wide showing `duration` seconds, clamped to the clip. */
export function pxToSecs(px: number, width: number, duration: number): number {
  if (width <= 0) return 0;
  return Math.min(Math.max((px / width) * duration, 0), duration);
}

export function secsToPx(secs: number, width: number, duration: number): number {
  if (duration <= 0) return 0;
  return (secs / duration) * width;
}

/** Moves one handle by `delta` seconds, keeping the range valid. */
export function nudge(range: Range, handle: Handle, delta: number, duration: number): Range {
  return handle === "start"
    ? clampRange(range.start + delta, range.end, duration, "start")
    : clampRange(range.start, range.end + delta, duration, "end");
}

/** Moves the whole range by `delta` seconds without changing its length, stopping at either edge. */
export function shiftRange(range: Range, delta: number, duration: number): Range {
  const len = range.end - range.start;
  const start = Math.min(Math.max(range.start + delta, 0), duration - len);
  return { start, end: start + len };
}
