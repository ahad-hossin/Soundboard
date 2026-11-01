import { render, screen, fireEvent } from "@testing-library/react";
import { DndContext } from "@dnd-kit/core";
import { SortableContext } from "@dnd-kit/sortable";
import { Tile } from "./Tile";
import type { UiSound } from "../lib/api";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => "asset://" + p,
  invoke: vi.fn(),
}));

const sound: UiSound = { id: "s1", name: "Airhorn", volume: 0.7, hotkey: "Ctrl+Shift+Digit1", color: "#FF595E", image_path: null, image_rev: 0, missing_file: false, duration_secs: 2.5, play_seq: 1 };

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

  it("fills the tile with its color when there is no image", () => {
    renderTile({ color: "#2EC4B6" });
    expect(screen.getByRole("button", { name: "Airhorn" })).toHaveStyle({ backgroundColor: "#2EC4B6" });
    expect(document.querySelector("span[aria-hidden='true']")).toBeNull();
  });

  it("decorates a no-image tile with gradient layers over its color", () => {
    renderTile({ color: "#2EC4B6" });
    const btn = screen.getByRole("button", { name: "Airhorn" });
    expect(btn.style.backgroundImage).toContain("radial-gradient");
    expect(btn).toHaveStyle({ backgroundColor: "#2EC4B6" });
  });

  it("does not decorate an image tile", () => {
    renderTile({ image_path: "C:/x/s1.png" });
    expect(screen.getByRole("button", { name: "Airhorn" }).style.backgroundImage).toBe("");
  });

  it("shows no sweep while idle", () => {
    renderTile();
    expect(screen.queryByTestId("sweep")).toBeNull();
  });

  it("sweeps over the clip duration while playing", () => {
    renderTile({ duration_secs: 2.5 }, true);
    const sweep = screen.getByTestId("sweep");
    expect(sweep.style.animationDuration).toBe("2.5s");
    expect(sweep).not.toHaveClass("sweep-pulse");
    // Fill sits in an inset track so the tile's rounded corner cannot clip it.
    const track = sweep.parentElement!;
    expect(track).toHaveClass("inset-x-2", "bottom-1.5", "h-0.5", "bg-white/20");
    expect(track).toHaveAttribute("aria-hidden", "true");
  });

  it("pulses an indeterminate bar when the duration is unknown", () => {
    renderTile({ duration_secs: null }, true);
    const sweep = screen.getByTestId("sweep");
    expect(sweep).toHaveClass("sweep-pulse");
    expect(sweep.style.animationDuration).toBe("");
  });

  it("restarts the sweep with a new node when play_seq changes", () => {
    const { rerender } = renderTile({ play_seq: 1 }, true);
    const first = screen.getByTestId("sweep");
    expect(first).toHaveAttribute("data-seq", "1");
    rerender({ play_seq: 2 });
    const second = screen.getByTestId("sweep");
    expect(second).toHaveAttribute("data-seq", "2");
    expect(second).not.toBe(first);
    rerender({ play_seq: 2 });
    expect(screen.getByTestId("sweep")).toBe(second);
  });

  it("shows the color as a 3px bottom bar on an image tile", () => {
    renderTile({ color: "#2EC4B6", image_path: "C:/x/s1.png" });
    const bar = document.querySelector("span[aria-hidden='true']");
    expect(bar).toHaveStyle({ backgroundColor: "#2EC4B6" });
    expect(bar).toHaveClass("bottom-0", "inset-x-0", "h-[3px]");
    expect(screen.getByRole("button", { name: "Airhorn" }).style.backgroundColor).toBe("");
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
