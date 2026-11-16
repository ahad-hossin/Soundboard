import { fireEvent, render, screen } from "@testing-library/react";
import { moveTo } from "../lib/actions";
import { api, type UiFolder, type UiSound, type UiState } from "../lib/api";
import { NO_SELECTION } from "../lib/selection";
import { useStore } from "../lib/store";
import { TileGrid } from "./TileGrid";

vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => "asset://" + p, invoke: vi.fn() }));
vi.mock("../lib/actions", () => ({ addPaths: vi.fn(), pickAndAddSounds: vi.fn(), pickImageFor: vi.fn(), moveTo: vi.fn() }));
vi.mock("../lib/api", () => ({
  api: { reorderSounds: vi.fn(), createFolder: vi.fn(), renameFolder: vi.fn(), deleteFolder: vi.fn(), deleteSound: vi.fn(), setSoundVolume: vi.fn() },
}));

const snd = (id: string, folder: string | null): UiSound => ({
  id, name: id.toUpperCase(), volume: 1, hotkey: null, color: "#FF595E", image_path: null, image_rev: 0,
  missing_file: false, duration_secs: null, play_seq: 0, trim: null, folder,
});

function setup(sounds: UiSound[], folders: UiFolder[]) {
  const state = {
    master_volume: 1,
    devices: { speakers: "Speakers", virtual_mic: null, microphone: null, mic_gain: 1, available: [], available_inputs: [] },
    stop_all_hotkey: "Ctrl+Shift+F12",
    startup: { autostart: false, start_minimized: false, close_to_tray: true },
    remote: { port: null, error: null, pin_set: false, addresses: [], clients: 0 },
    sounds, folders, playing: [], startup_notices: [],
  } satisfies UiState;
  const play = vi.fn();
  useStore.setState({ state, openFolder: null, sel: NO_SELECTION, optimistic: new Set(), pendingTrimId: null, play, run: async (p) => { await p; } });
  render(<TileGrid />);
  return { play };
}

const labels = () => screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"));
const sounds = [snd("a", null), snd("b", "f1"), snd("c", null), snd("d", "f1")];
const folders = [{ id: "f1", name: "Memes" }];

describe("TileGrid", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows folder tiles first, then top-level sounds, then add tiles", () => {
    setup(sounds, folders);
    expect(labels()).toEqual(["Folder Memes, 2 sounds", "A", "C", "Add sounds", "New folder"]);
  });

  it("opens a folder and goes back", () => {
    setup(sounds, folders);
    fireEvent.click(screen.getByRole("button", { name: "Folder Memes, 2 sounds" }));
    expect(labels()).toEqual(["Back to top level", "B", "D", "Add sounds"]);
    fireEvent.click(screen.getByRole("button", { name: "Back to top level" }));
    expect(screen.getByRole("button", { name: "A" })).toBeInTheDocument();
  });

  it("Ctrl+click selects without playing; the bar moves the whole selection in one call", () => {
    const { play } = setup(sounds, folders);
    fireEvent.click(screen.getByRole("button", { name: "A" }), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "C" }), { ctrlKey: true });
    expect(play).not.toHaveBeenCalled();
    expect(screen.getByText("2 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "A, selected" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Move to folder…" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Memes" }));
    expect(moveTo).toHaveBeenCalledWith(["a", "c"], "f1");
  });

  it("the move menu leaves out where the sounds already are", () => {
    setup(sounds, folders);
    fireEvent.contextMenu(screen.getByRole("button", { name: "A" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Move to folder…" }));
    expect(screen.queryByRole("menuitem", { name: "Top level" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Memes" })).toBeInTheDocument();
  });

  it("the context menu on a selected sound acts on the whole selection", () => {
    setup(sounds, folders);
    fireEvent.click(screen.getByRole("button", { name: "A" }), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "C" }), { ctrlKey: true });
    fireEvent.contextMenu(screen.getByRole("button", { name: "C, selected" }));
    expect(screen.getByRole("menuitem", { name: "Move 2 sounds to folder…" })).toBeInTheDocument();
  });

  it("offers no move when there is nowhere to go", () => {
    setup([snd("a", null)], []);
    fireEvent.contextMenu(screen.getByRole("button", { name: "A" }));
    expect(screen.queryByRole("menuitem", { name: "Move to folder…" })).toBeNull();
  });

  it("deleting a folder asks first and says sounds are kept", () => {
    setup(sounds, folders);
    fireEvent.contextMenu(screen.getByRole("button", { name: "Folder Memes, 2 sounds" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(screen.getByText(/Sounds inside move to the top level/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(api.deleteFolder).toHaveBeenCalledWith("f1");
  });

  it("New folder tile creates a folder from the name dialog", () => {
    setup(sounds, folders);
    fireEvent.click(screen.getByRole("button", { name: "New folder" }));
    // getByLabelText("New folder") would also match the tile's aria-label.
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Music" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(api.createFolder).toHaveBeenCalledWith("Music");
  });

  it("shows the grid, not the empty state, when there are folders but no sounds", () => {
    setup([], folders);
    expect(screen.getByRole("button", { name: "Folder Memes, 0 sounds" })).toBeInTheDocument();
  });
});
