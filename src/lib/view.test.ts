import type { UiSound } from "./api";
import { summarize, viewSounds } from "./view";

const s = (id: string, folder: string | null, color = "#FF595E"): UiSound => ({
  id, name: id, volume: 1, hotkey: null, color, image_path: null, image_rev: 0,
  missing_file: false, duration_secs: null, play_seq: 0, trim: null, folder,
});

const sounds = [s("a", null), s("b", "f1", "#111111"), s("c", null), s("d", "f1", "#222222")];

describe("view", () => {
  it("viewSounds keeps the global order and filters by folder", () => {
    expect(viewSounds(sounds, null).map((x) => x.id)).toEqual(["a", "c"]);
    expect(viewSounds(sounds, "f1").map((x) => x.id)).toEqual(["b", "d"]);
  });

  it("summarize counts, previews up to four colors and reports playing", () => {
    expect(summarize(sounds, "f1", new Set())).toEqual({ count: 2, colors: ["#111111", "#222222"], playing: false });
    expect(summarize(sounds, "f1", new Set(["d"])).playing).toBe(true);
    const many = ["1", "2", "3", "4", "5"].map((id) => s(id, "f9"));
    expect(summarize(many, "f9", new Set()).colors).toHaveLength(4);
  });
});
