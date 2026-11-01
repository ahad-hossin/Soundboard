import { PALETTE } from "./palette";

describe("PALETTE", () => {
  it("has ten distinct uppercase #RRGGBB colors", () => {
    expect(PALETTE).toHaveLength(10);
    for (const c of PALETTE) expect(c).toMatch(/^#[0-9A-F]{6}$/);
    expect(new Set(PALETTE).size).toBe(10);
  });
});
