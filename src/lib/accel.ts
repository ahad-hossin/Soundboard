const MOD_CODES = new Set([
  "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight",
  "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

export interface KeyLike {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}

const F_KEY = /^F([1-9]|1[0-9]|2[0-4])$/;

/** Builds the canonical accelerator for a keydown, or null if it is not bindable. */
export function accelFromEvent(e: KeyLike): string | null {
  if (!e.code || MOD_CODES.has(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (mods.length === 0 && !F_KEY.test(e.code)) return null;
  return [...mods, e.code].join("+");
}

const NAMES: Record<string, string> = {
  Escape: "Esc", Enter: "Enter", Space: "Space", Tab: "Tab", Backspace: "Bksp", Delete: "Del",
  Insert: "Ins", Home: "Home", End: "End", PageUp: "PgUp", PageDown: "PgDn",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→",
  Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\",
  Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/", Backquote: "`",
};

/** Human form of an accelerator for badges: Ctrl+Shift+Digit1 -> Ctrl+Shift+1 */
export function displayAccel(accel: string | null | undefined): string {
  if (!accel) return "";
  return accel
    .split("+")
    .map((p) => {
      if (/^Key[A-Z]$/.test(p)) return p[3];
      if (/^Digit[0-9]$/.test(p)) return p[5];
      if (p.startsWith("Numpad")) return "Num" + p.slice(6);
      return NAMES[p] ?? p;
    })
    .join("+");
}
