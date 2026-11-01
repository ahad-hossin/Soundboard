/** `m:ss.mmm`, minutes unpadded, rounded to the millisecond. Negative or non-finite becomes 0. */
export function formatTime(secs: number): string {
  const ms = Number.isFinite(secs) && secs > 0 ? Math.round(secs * 1000) : 0;
  const m = Math.floor(ms / 60000);
  const s = Math.floor((ms % 60000) / 1000);
  const frac = ms % 1000;
  return `${m}:${String(s).padStart(2, "0")}.${String(frac).padStart(3, "0")}`;
}

/** Parses `m:ss.mmm`, `m:ss`, `ss.mmm` or plain seconds. Seconds after a colon must be below 60. */
export function parseTime(text: string): number | null {
  const m = /^\s*(?:(\d+):)?(\d+(?:\.\d*)?|\.\d+)\s*$/.exec(text);
  if (!m) return null;
  const secs = Number(m[2]);
  if (!Number.isFinite(secs)) return null;
  if (m[1] === undefined) return secs;
  if (secs >= 60) return null;
  return Number(m[1]) * 60 + secs;
}
