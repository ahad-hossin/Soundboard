import { motifIndex, tileDecoration } from "./decor";

describe("motifIndex", () => {
  it("is stable for an id", () => {
    expect(motifIndex("sound-1")).toBe(motifIndex("sound-1"));
  });

  it("is the char code sum mod 4", () => {
    expect(motifIndex("a")).toBe(97 % 4);
    expect(motifIndex("ab")).toBe((97 + 98) % 4);
    expect(motifIndex("")).toBe(0);
  });

  it("spans 0..3 over sample ids", () => {
    const seen = new Set(["a", "b", "c", "d", "s1", "s2", "s3", "s4"].map(motifIndex));
    expect([...seen].sort()).toEqual([0, 1, 2, 3]);
  });
});

describe("tileDecoration", () => {
  it("puts the color under every gradient layer", () => {
    const s = tileDecoration("s1", "#2EC4B6");
    expect(s.backgroundColor).toBe("#2EC4B6");
    expect(s.backgroundImage).toContain("radial-gradient(circle at 25% 20%");
  });

  it("keeps image, size, position and repeat layers aligned", () => {
    for (const id of ["a", "b", "c", "d"]) {
      const s = tileDecoration(id, "#FF595E");
      expect(String(s.backgroundSize).split(/,(?![^(]*\))/)).toHaveLength(2);
      expect(String(s.backgroundPosition).split(",")).toHaveLength(2);
      expect(String(s.backgroundRepeat).split(",")).toHaveLength(2);
    }
  });

  it("differs by motif", () => {
    const images = new Set(["a", "b", "c", "d"].map((id) => tileDecoration(id, "#FF595E").backgroundImage));
    expect(images.size).toBe(4);
  });
});
