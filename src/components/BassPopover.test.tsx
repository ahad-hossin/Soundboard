import { fireEvent, render, screen } from "@testing-library/react";
import { BassPopover } from "./BassPopover";

describe("BassPopover", () => {
  it("shows the strength and commits on release", () => {
    const onCommit = vi.fn();
    render(<BassPopover x={10} y={10} name="Airhorn" value={0.4} onCommit={onCommit} onClose={() => {}} />);
    expect(screen.getByText("40%")).toBeInTheDocument();
    const slider = screen.getByLabelText("Airhorn bass boost");
    fireEvent.change(slider, { target: { value: "0.75" } });
    expect(screen.getByText("75%")).toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    expect(onCommit).toHaveBeenCalledWith(0.75);
  });
});
