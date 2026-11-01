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

function tree(s: UiSound, playing: boolean, onPlay: () => void, onVolume: (v: number) => void) {
  return (
    <DndContext>
      <SortableContext items={[s.id]}>
        <Tile sound={s} playing={playing} onPlay={onPlay} onVolume={onVolume} onContextMenu={() => {}} />
      </SortableContext>
    </DndContext>
  );
}

function renderTile(over: Partial<UiSound> = {}, playing = false, onPlay = vi.fn(), onVolume = vi.fn()) {
  const s = { ...sound, ...over };
  const { rerender } = render(tree(s, playing, onPlay, onVolume));
  return { onPlay, onVolume, rerender: (o: Partial<UiSound>) => rerender(tree({ ...s, ...o }, playing, onPlay, onVolume)) };
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
    const slider = screen.getByLabelText("Airhorn volume");
    fireEvent.change(slider, { target: { value: "0.3" } });
    expect(onVolume).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    expect(onVolume).toHaveBeenCalledWith(0.3);
    expect(onVolume).toHaveBeenCalledTimes(1);
  });

  it("commits keyboard changes on key up and ignores a bare blur", () => {
    const { onVolume } = renderTile();
    const slider = screen.getByLabelText("Airhorn volume");
    fireEvent.blur(slider);
    expect(onVolume).not.toHaveBeenCalled();
    fireEvent.change(slider, { target: { value: "0.5" } });
    fireEvent.keyUp(slider, { key: "ArrowRight" });
    expect(onVolume).toHaveBeenCalledWith(0.5);
  });

  it("keeps the dragged value until release, then follows props", () => {
    const { rerender } = renderTile();
    const slider = screen.getByLabelText("Airhorn volume") as HTMLInputElement;
    fireEvent.change(slider, { target: { value: "0.2" } });
    rerender({ volume: 0.9 });
    expect(slider.value).toBe("0.2");
    fireEvent.pointerUp(slider);
    rerender({ volume: 0.4 });
    expect(slider.value).toBe("0.4");
  });

  it("slider is skipped by Tab", () => {
    renderTile();
    expect(screen.getByLabelText("Airhorn volume")).toHaveAttribute("tabindex", "-1");
  });
});
