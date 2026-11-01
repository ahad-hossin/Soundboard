import { act, fireEvent, render, screen } from "@testing-library/react";
import { api, type UiSound, type Waveform } from "../lib/api";
import { useStore } from "../lib/store";
import { TrimEditor } from "./TrimEditor";

vi.mock("../lib/api", () => ({
  api: {
    getWaveform: vi.fn(),
    setTrim: vi.fn(),
    previewTrim: vi.fn(),
    stopPreview: vi.fn(),
  },
}));

const mocked = api as unknown as Record<keyof typeof api, ReturnType<typeof vi.fn>>;

const sound: UiSound = { id: "s1", name: "Airhorn", volume: 1, hotkey: null, color: "#FF595E", image_path: null, image_rev: 0, missing_file: false, duration_secs: 4, play_seq: 0, trim: null };

function wave(over: Partial<Waveform> = {}): Waveform {
  return { duration_secs: 4, peaks: Array.from({ length: 600 }, (_, i) => (i % 10) / 10), trim: null, ...over };
}

async function open(mode: "edit" | "import" = "edit", w: Waveform = wave()) {
  mocked.getWaveform.mockResolvedValue(w);
  const onClose = vi.fn();
  render(<TrimEditor sound={sound} mode={mode} onClose={onClose} />);
  await screen.findByRole("button", { name: "Play selection" });
  return onClose;
}

describe("TrimEditor", () => {
  beforeAll(() => {
    // jsdom has no canvas; drawing calls go to a no-op stub.
    const stub = new Proxy({}, { get: (_t, key) => (key === "canvas" ? undefined : () => {}), set: () => true });
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(() => stub as never);
  });

  beforeEach(() => {
    vi.clearAllMocks();
    mocked.setTrim.mockResolvedValue(undefined);
    mocked.previewTrim.mockResolvedValue(undefined);
    mocked.stopPreview.mockResolvedValue(undefined);
    useStore.setState({ toasts: [] });
  });

  it("shows Loading until the waveform arrives, then the name, fields and buttons", async () => {
    mocked.getWaveform.mockReturnValue(new Promise(() => {}));
    const { unmount } = render(<TrimEditor sound={sound} mode="edit" onClose={vi.fn()} />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    unmount();

    await open();
    expect(screen.getByText("Airhorn")).toBeInTheDocument();
    expect(mocked.getWaveform).toHaveBeenCalledWith("s1");
    expect(screen.getByLabelText("Start")).toHaveValue("0:00.000");
    expect(screen.getByLabelText("End")).toHaveValue("0:04.000");
    for (const name of ["Play selection", "Reset", "Cancel", "Save"]) expect(screen.getByRole("button", { name })).toBeInTheDocument();
  });

  it("Save stores the edited range", async () => {
    const onClose = await open();
    const start = screen.getByLabelText("Start");
    fireEvent.change(start, { target: { value: "0:01.250" } });
    fireEvent.blur(start);
    const end = screen.getByLabelText("End");
    fireEvent.change(end, { target: { value: "3.5" } });
    fireEvent.keyDown(end, { key: "Enter" });
    fireEvent.blur(end);
    expect(screen.getByLabelText("Selection length")).toHaveTextContent("0:02.250");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(mocked.setTrim).toHaveBeenCalledWith("s1", { start_secs: 1.25, end_secs: 3.5 });
    expect(onClose).toHaveBeenCalled();
  });

  it("Reset then Save clears an existing trim", async () => {
    const onClose = await open("edit", wave({ trim: { start_secs: 0.5, end_secs: 2 } }));
    expect(screen.getByLabelText("Start")).toHaveValue("0:00.500");
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(screen.getByLabelText("End")).toHaveValue("0:04.000");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(mocked.setTrim).toHaveBeenCalledWith("s1", null);
    expect(onClose).toHaveBeenCalled();
  });

  it("Skip in import mode only closes", async () => {
    const onClose = await open("import");
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(mocked.setTrim).not.toHaveBeenCalled();
    expect(mocked.stopPreview).not.toHaveBeenCalled();
  });

  it("Esc stops the preview and closes", async () => {
    const onClose = await open();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(mocked.stopPreview).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    expect(mocked.setTrim).not.toHaveBeenCalled();
  });

  it("Play selection previews the range and toggles to Stop", async () => {
    await open("edit", wave({ trim: { start_secs: 1, end_secs: 2 } }));
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Play selection" })); });
    expect(mocked.previewTrim).toHaveBeenCalledWith("s1", 1, 2);
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(mocked.stopPreview).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Play selection" })).toBeInTheDocument();
  });

  it("arrow keys nudge the focused handle by 10 ms, Shift by 100 ms, never past the other handle", async () => {
    await open("edit", wave({ trim: { start_secs: 1, end_secs: 2 } }));
    const start = screen.getByRole("slider", { name: "Trim start" });
    fireEvent.keyDown(start, { key: "ArrowRight" });
    expect(screen.getByLabelText("Start")).toHaveValue("0:01.010");
    fireEvent.keyDown(start, { key: "ArrowLeft", shiftKey: true });
    expect(screen.getByLabelText("Start")).toHaveValue("0:00.910");
    const end = screen.getByRole("slider", { name: "Trim end" });
    for (let i = 0; i < 20; i++) fireEvent.keyDown(end, { key: "ArrowLeft", shiftKey: true });
    expect(screen.getByLabelText("End")).toHaveValue("0:00.960");
  });

  it("an unparsable time is ignored", async () => {
    await open();
    const start = screen.getByLabelText("Start");
    fireEvent.change(start, { target: { value: "soon" } });
    fireEvent.blur(start);
    expect(start).toHaveValue("0:00.000");
  });
});
