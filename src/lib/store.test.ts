import { useStore } from "./store";

vi.mock("./api", () => ({
  api: {
    play: vi.fn(),
    getState: vi.fn(),
    onState: vi.fn(),
    onToast: vi.fn(),
  },
}));

describe("store", () => {
  beforeEach(() => {
    useStore.setState({ toasts: [], panel: null, optimistic: new Set() });
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("keeps at most three toasts and auto-dismisses after 4s", () => {
    const { toast } = useStore.getState();
    toast("info", "a");
    toast("info", "b");
    toast("info", "c");
    toast("info", "d");
    expect(useStore.getState().toasts.map((t) => t.message)).toEqual(["b", "c", "d"]);
    vi.advanceTimersByTime(4000);
    expect(useStore.getState().toasts).toHaveLength(0);
  });

  it("run() turns a rejection into an error toast", async () => {
    await useStore.getState().run(Promise.reject("Port must be between 1024 and 65535"));
    expect(useStore.getState().toasts[0]).toMatchObject({ level: "error", message: "Port must be between 1024 and 65535" });
  });

  it("togglePanel opens and closes", () => {
    const { togglePanel } = useStore.getState();
    togglePanel("settings");
    expect(useStore.getState().panel).toBe("settings");
    togglePanel("settings");
    expect(useStore.getState().panel).toBeNull();
    togglePanel("settings");
    togglePanel("remote");
    expect(useStore.getState().panel).toBe("remote");
  });

  it("play() marks optimistic then clears", async () => {
    const { api } = await import("./api");
    (api.play as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    const p = useStore.getState().play("s1");
    expect(useStore.getState().optimistic.has("s1")).toBe(true);
    await p;
    vi.advanceTimersByTime(200);
    expect(useStore.getState().optimistic.has("s1")).toBe(false);
  });
});
