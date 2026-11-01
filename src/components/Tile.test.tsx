import { render, screen, fireEvent } from "@testing-library/react";
import { DndContext } from "@dnd-kit/core";
import { SortableContext } from "@dnd-kit/sortable";
import { Tile } from "./Tile";
import type { UiSound } from "../lib/api";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => "asset://" + p,
  invoke: vi.fn(),
}));

const sound: UiSound = { id: "s1", name: "Airhorn", volume: 0.7, hotkey: "Ctrl+Shift+Digit1", image_path: null, image_rev: 0, missing_file: false };

function renderTile(over: Partial<UiSound> = {}, playing = false, onPlay = vi.fn(), onVolume = vi.fn()) {
  const s = { ...sound, ...over };
  render(
    <DndContext>
      <SortableContext items={[s.id]}>
        <Tile sound={s} playing={playing} onPlay={onPlay} onVolume={onVolume} onContextMenu={() => {}} />
      </SortableContext>
    </DndContext>,
  );
  return { onPlay, onVolume };
}

describe("Tile", () => {
  it("shows name and shortened hotkey badge", () => {
    renderTile();
    expect(screen.getByText("Airhorn")).toBeInTheDocument();
    expect(screen.getByText("Ctrl+Shift+1")).toBeInTheDocument();
  });

  it("marks playing state on the button", () => {
    renderTile({}, true);
    expect(screen.getByRole("button", { name: "Airhorn" })).toHaveAttribute("aria-pressed", "true");
  });

  it("renders the image when present", () => {
    renderTile({ image_path: "C:/x/s1.png" });
    expect(document.querySelector("img")?.getAttribute("src")).toBe("asset://C:/x/s1.png?v=0");
  });

  it("flags a missing file", () => {
    renderTile({ missing_file: true });
    expect(screen.getByTitle("File missing")).toBeInTheDocument();
  });

  it("click plays, slider sets volume", () => {
    const { onPlay, onVolume } = renderTile();
    fireEvent.click(screen.getByRole("button", { name: "Airhorn" }));
    expect(onPlay).toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("Airhorn volume"), { target: { value: "0.3" } });
    expect(onVolume).toHaveBeenCalledWith(0.3);
  });
});
