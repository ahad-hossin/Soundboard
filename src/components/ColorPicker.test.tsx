import { render, screen, fireEvent } from "@testing-library/react";
import { ColorPicker } from "./ColorPicker";
import { PALETTE } from "../lib/palette";

describe("ColorPicker", () => {
  it("renders one swatch per palette color and marks the current one", () => {
    render(<ColorPicker x={10} y={10} current="#2ec4b6" onPick={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getAllByRole("button")).toHaveLength(10);
    for (const c of PALETTE) {
      expect(screen.getByRole("button", { name: c })).toHaveAttribute("aria-pressed", c === "#2EC4B6" ? "true" : "false");
    }
    expect(screen.getByRole("button", { name: "#FF595E" })).toHaveStyle({ backgroundColor: "#FF595E" });
  });

  it("picking calls onPick then onClose", () => {
    const calls: string[] = [];
    const onPick = vi.fn((c: string) => calls.push(`pick:${c}`));
    const onClose = vi.fn(() => calls.push("close"));
    render(<ColorPicker x={0} y={0} current="#FF595E" onPick={onPick} onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "#6A4C93" }));
    expect(calls).toEqual(["pick:#6A4C93", "close"]);
  });

  it("closes on Escape without reaching the global Stop All handler, and on an outside click", () => {
    const onClose = vi.fn();
    const globalHandler = vi.fn();
    window.addEventListener("keydown", globalHandler);
    render(<ColorPicker x={0} y={0} current="#FF595E" onPick={vi.fn()} onClose={onClose} />);
    fireEvent.keyDown(document.body, { key: "Escape" });
    window.removeEventListener("keydown", globalHandler);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(globalHandler).not.toHaveBeenCalled();
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
