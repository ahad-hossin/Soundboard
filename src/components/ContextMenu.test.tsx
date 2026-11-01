import { render, screen, fireEvent } from "@testing-library/react";
import { ContextMenu } from "./ContextMenu";

describe("ContextMenu", () => {
  it("renders items and calls onSelect then onClose", () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<ContextMenu x={10} y={10} onClose={onClose} items={[{ label: "Rename", onSelect }, { label: "Delete", onSelect: vi.fn(), danger: true }]} />);
    fireEvent.click(screen.getByText("Rename"));
    expect(onSelect).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    expect(screen.getByText("Delete")).toHaveClass("text-danger");
  });

  it("closes on Escape without reaching the global Stop All handler", () => {
    const onClose = vi.fn();
    const globalHandler = vi.fn();
    window.addEventListener("keydown", globalHandler);
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "A", onSelect: vi.fn() }]} />);
    fireEvent.keyDown(document.body, { key: "Escape" });
    window.removeEventListener("keydown", globalHandler);
    expect(onClose).toHaveBeenCalled();
    expect(globalHandler).not.toHaveBeenCalled();
  });
});
