import { api } from "./api";
import { NO_SELECTION } from "./selection";
import { handleGlobalKey } from "./shortcuts";
import { useStore } from "./store";

vi.mock("./api", () => ({ api: { stopAll: vi.fn().mockResolvedValue(undefined) } }));

describe("handleGlobalKey", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStore.setState({ sel: NO_SELECTION });
  });

  it("Esc clears a selection first and does not stop sounds", () => {
    useStore.setState({ sel: new Set(["a"]) });
    handleGlobalKey({ key: "Escape", target: document.body });
    expect(useStore.getState().sel.size).toBe(0);
    expect(api.stopAll).not.toHaveBeenCalled();
  });

  it("Esc with nothing selected stops all", () => {
    handleGlobalKey({ key: "Escape", target: document.body });
    expect(api.stopAll).toHaveBeenCalledTimes(1);
  });

  it("does nothing while typing in a field", () => {
    handleGlobalKey({ key: "Escape", target: document.createElement("input") });
    expect(api.stopAll).not.toHaveBeenCalled();
  });
});
