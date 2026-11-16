import { NO_SELECTION, targetIds, toggle } from "./selection";

describe("selection", () => {
  it("toggle adds and removes without touching the input", () => {
    const one = toggle(NO_SELECTION, "b");
    expect([...one]).toEqual(["b"]);
    expect([...toggle(one, "b")]).toEqual([]);
    expect(NO_SELECTION.size).toBe(0);
  });

  it("targetIds is the whole selection in order when the sound is selected, else just that sound", () => {
    const sel = toggle(toggle(NO_SELECTION, "d"), "a");
    expect(targetIds(sel, "d", ["a", "b", "c", "d"])).toEqual(["a", "d"]);
    expect(targetIds(sel, "c", ["a", "b", "c", "d"])).toEqual(["c"]);
  });
});
