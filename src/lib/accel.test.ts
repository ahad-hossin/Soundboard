import { accelFromEvent, displayAccel } from "./accel";

const ev = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
  ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, code, ...mods,
});

describe("accelFromEvent", () => {
  it("joins modifiers in fixed order with the code", () => {
    expect(accelFromEvent(ev("Digit1", { shiftKey: true, ctrlKey: true }))).toBe("Ctrl+Shift+Digit1");
    expect(accelFromEvent(ev("KeyZ", { metaKey: true, altKey: true }))).toBe("Alt+Super+KeyZ");
  });
  it("ignores bare modifier presses", () => {
    expect(accelFromEvent(ev("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(accelFromEvent(ev("ShiftRight", { shiftKey: true }))).toBeNull();
  });
  it("requires a modifier except for F-keys", () => {
    expect(accelFromEvent(ev("KeyA"))).toBeNull();
    expect(accelFromEvent(ev("F9"))).toBe("F9");
    expect(accelFromEvent(ev("F24"))).toBe("F24");
  });
  it("returns null for empty code", () => {
    expect(accelFromEvent(ev(""))).toBeNull();
  });
});

describe("displayAccel", () => {
  it("shortens key codes", () => {
    expect(displayAccel("Ctrl+Shift+Digit1")).toBe("Ctrl+Shift+1");
    expect(displayAccel("Alt+KeyQ")).toBe("Alt+Q");
    expect(displayAccel("Ctrl+Shift+F12")).toBe("Ctrl+Shift+F12");
    expect(displayAccel("Ctrl+Escape")).toBe("Ctrl+Esc");
    expect(displayAccel("Ctrl+Numpad5")).toBe("Ctrl+Num5");
    expect(displayAccel("Ctrl+ArrowUp")).toBe("Ctrl+↑");
    expect(displayAccel("Ctrl+BracketLeft")).toBe("Ctrl+[");
  });
  it("handles empty", () => {
    expect(displayAccel(null)).toBe("");
    expect(displayAccel(undefined)).toBe("");
  });
});
