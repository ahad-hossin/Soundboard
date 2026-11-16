import { fireEvent, render, screen } from "@testing-library/react";
import { BackTile, FolderTile, NewFolderTile } from "./FolderTiles";

describe("FolderTiles", () => {
  it("folder tile shows name and count, opens on click, and rings while playing", () => {
    const onOpen = vi.fn();
    render(<FolderTile folder={{ id: "f1", name: "Memes" }} summary={{ count: 2, colors: ["#FF595E"], playing: true }} onOpen={onOpen} onContextMenu={vi.fn()} />);
    const btn = screen.getByRole("button", { name: "Folder Memes, 2 sounds" });
    expect(btn).toHaveClass("ring-2");
    fireEvent.click(btn);
    expect(onOpen).toHaveBeenCalled();
  });

  it("back and new folder tiles call their handlers", () => {
    const onBack = vi.fn();
    const onNew = vi.fn();
    render(<><BackTile onBack={onBack} /><NewFolderTile onClick={onNew} /></>);
    fireEvent.click(screen.getByRole("button", { name: "Back to top level" }));
    fireEvent.click(screen.getByRole("button", { name: "New folder" }));
    expect(onBack).toHaveBeenCalled();
    expect(onNew).toHaveBeenCalled();
  });
});
