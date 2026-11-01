import { clampRange, MIN_GAP, nudge, pxToSecs, secsToPx, shiftRange } from "./trimMath";

describe("trimMath", () => {
  it("clampRange keeps the range inside the clip", () => {
    expect(clampRange(-1, 12, 10)).toEqual({ start: 0, end: 10 });
    expect(clampRange(2, 5, 10)).toEqual({ start: 2, end: 5 });
  });

  it("clampRange pushes the moving handle back to keep the minimum gap", () => {
    expect(clampRange(4.99, 5, 10, "start")).toEqual({ start: 5 - MIN_GAP, end: 5 });
    expect(clampRange(5, 5.01, 10, "end")).toEqual({ start: 5, end: 5 + MIN_GAP });
    expect(clampRange(9.99, 10, 10, "end").start).toBeCloseTo(10 - MIN_GAP);
    expect(clampRange(0, 0.01, 10, "start")).toEqual({ start: 0, end: MIN_GAP });
  });

  it("pxToSecs and secsToPx convert and clamp", () => {
    expect(pxToSecs(264, 528, 4)).toBe(2);
    expect(pxToSecs(-10, 528, 4)).toBe(0);
    expect(pxToSecs(999, 528, 4)).toBe(4);
    expect(pxToSecs(10, 0, 4)).toBe(0);
    expect(secsToPx(1, 528, 4)).toBe(132);
    expect(secsToPx(1, 528, 0)).toBe(0);
  });

  it("nudge moves one handle and respects the bounds and gap", () => {
    const r = { start: 1, end: 2 };
    expect(nudge(r, "start", 0.01, 4).start).toBeCloseTo(1.01);
    expect(nudge(r, "end", -0.1, 4).end).toBeCloseTo(1.9);
    expect(nudge(r, "start", 5, 4)).toEqual({ start: 2 - MIN_GAP, end: 2 });
    expect(nudge(r, "end", 5, 4)).toEqual({ start: 1, end: 4 });
    expect(nudge(r, "start", -5, 4)).toEqual({ start: 0, end: 2 });
  });

  it("shiftRange keeps the length and stops at the edges", () => {
    expect(shiftRange({ start: 1, end: 2 }, 0.5, 4)).toEqual({ start: 1.5, end: 2.5 });
    expect(shiftRange({ start: 1, end: 2 }, 5, 4)).toEqual({ start: 3, end: 4 });
    expect(shiftRange({ start: 1, end: 2 }, -5, 4)).toEqual({ start: 0, end: 1 });
  });
});
