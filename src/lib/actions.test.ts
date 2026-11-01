import { addPaths } from "./actions";
import { api } from "./api";
import { useStore } from "./store";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./api", () => ({ api: { addSounds: vi.fn() } }));

const addSounds = api.addSounds as ReturnType<typeof vi.fn>;

describe("addPaths", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStore.setState({ toasts: [], pendingTrimId: null });
  });

  it("opens the trim editor when exactly one sound was added", async () => {
    addSounds.mockResolvedValue({ rejected: [], added_ids: ["s1"] });
    await addPaths(["C:/a.wav"]);
    expect(useStore.getState().pendingTrimId).toBe("s1");
  });

  it("never opens it for several sounds, and toasts each rejection", async () => {
    addSounds.mockResolvedValue({ rejected: ["b.txt is not a supported audio file"], added_ids: ["s1", "s2"] });
    await addPaths(["C:/a.wav", "C:/c.wav", "C:/b.txt"]);
    expect(useStore.getState().pendingTrimId).toBeNull();
    expect(useStore.getState().toasts.map((t) => t.message)).toEqual(["b.txt is not a supported audio file"]);
  });

  it("turns a failed import into an error toast", async () => {
    addSounds.mockRejectedValue("Importing sounds failed");
    await addPaths(["C:/a.wav"]);
    expect(useStore.getState().toasts[0]).toMatchObject({ level: "error", message: "Importing sounds failed" });
    expect(useStore.getState().pendingTrimId).toBeNull();
  });
});
