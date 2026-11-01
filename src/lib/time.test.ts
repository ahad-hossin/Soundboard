import { formatTime, parseTime } from "./time";

describe("formatTime", () => {
  it("formats minutes, padded seconds and milliseconds", () => {
    expect(formatTime(0)).toBe("0:00.000");
    expect(formatTime(1.5)).toBe("0:01.500");
    expect(formatTime(75.0123)).toBe("1:15.012");
    expect(formatTime(600)).toBe("10:00.000");
  });

  it("rounds to the millisecond, carrying into seconds", () => {
    expect(formatTime(59.9996)).toBe("1:00.000");
  });

  it("treats negative and non-finite as zero", () => {
    expect(formatTime(-3)).toBe("0:00.000");
    expect(formatTime(NaN)).toBe("0:00.000");
  });
});

describe("parseTime", () => {
  it("parses the formatted form and shorter forms", () => {
    expect(parseTime("1:15.012")).toBeCloseTo(75.012);
    expect(parseTime("0:01.5")).toBe(1.5);
    expect(parseTime("2:05")).toBe(125);
    expect(parseTime("3.25")).toBe(3.25);
    expect(parseTime(" 90 ")).toBe(90);
    expect(parseTime(".5")).toBe(0.5);
  });

  it("round-trips formatTime", () => {
    for (const t of [0, 0.05, 12.345, 61.001]) expect(parseTime(formatTime(t))).toBeCloseTo(t, 3);
  });

  it("rejects malformed input and seconds of 60 or more after a colon", () => {
    for (const bad of ["", "abc", "1:2:3", "-1", "1:60", "1:75.5", "1:", ":30"]) expect(parseTime(bad)).toBeNull();
  });
});
