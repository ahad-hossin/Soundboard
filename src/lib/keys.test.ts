import { isTypingTarget } from "./keys";

function input(type: string) {
  const el = document.createElement("input");
  el.type = type;
  return el;
}

describe("isTypingTarget", () => {
  it("is true for text-like fields", () => {
    expect(isTypingTarget(input("text"))).toBe(true);
    expect(isTypingTarget(document.createElement("textarea"))).toBe(true);
    expect(isTypingTarget(document.createElement("select"))).toBe(true);
  });

  it("is false for range, checkbox, radio and button inputs", () => {
    expect(isTypingTarget(input("range"))).toBe(false);
    expect(isTypingTarget(input("checkbox"))).toBe(false);
    expect(isTypingTarget(input("radio"))).toBe(false);
    expect(isTypingTarget(input("button"))).toBe(false);
  });

  it("is false for buttons and null", () => {
    expect(isTypingTarget(document.createElement("button"))).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});
