import { render, screen } from "@testing-library/react";
import { ConfirmDialog } from "./Dialogs";

describe("ConfirmDialog", () => {
  it("focuses Cancel, not the destructive action", () => {
    render(<ConfirmDialog title="Delete it?" confirmLabel="Delete" danger onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "Delete" })).toHaveClass("text-text");
  });
});
