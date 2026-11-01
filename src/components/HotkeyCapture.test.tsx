import { render, screen, fireEvent } from "@testing-library/react";
import { HotkeyCapture } from "./HotkeyCapture";

describe("HotkeyCapture", () => {
  it("captures a combo and commits it", () => {
    const onCapture = vi.fn();
    render(<HotkeyCapture title="Hotkey" current={null} onCapture={onCapture} onCancel={vi.fn()} />);
    fireEvent.keyDown(window, { code: "Digit2", ctrlKey: true, shiftKey: true });
    expect(onCapture).toHaveBeenCalledWith("Ctrl+Shift+Digit2");
  });

  it("ignores a bare letter and shows the hint", () => {
    const onCapture = vi.fn();
    render(<HotkeyCapture title="Hotkey" current={null} onCapture={onCapture} onCancel={vi.fn()} />);
    fireEvent.keyDown(window, { code: "KeyA" });
    expect(onCapture).not.toHaveBeenCalled();
    expect(screen.getByText(/needs a modifier/i)).toBeInTheDocument();
  });

  it("plain Escape cancels, Clear calls onClear", () => {
    const onCancel = vi.fn();
    const onClear = vi.fn();
    render(<HotkeyCapture title="Hotkey" current="Alt+KeyQ" onCapture={vi.fn()} onClear={onClear} onCancel={onCancel} />);
    expect(screen.getByText("Alt+Q")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Clear"));
    expect(onClear).toHaveBeenCalled();
    fireEvent.keyDown(window, { code: "Escape", key: "Escape" });
    expect(onCancel).toHaveBeenCalled();
  });
});
