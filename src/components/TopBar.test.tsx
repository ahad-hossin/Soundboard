import { fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "../lib/store";
import { api } from "../lib/api";
import { TopBar } from "./TopBar";

vi.mock("../lib/api", () => ({
  api: {
    stopAll: vi.fn(),
    setMasterVolume: vi.fn(),
    openSupportPage: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock("../lib/actions", () => ({ pickAndAddSounds: vi.fn() }));

const base = {
  master_volume: 0.8,
  devices: { speakers: null, virtual_mic: null, microphone: null, mic_gain: 1, available: [], available_inputs: [] },
  stop_all_hotkey: "Ctrl+Shift+F12",
  startup: { autostart: false, start_minimized: false, close_to_tray: true },
  remote: { port: 7331, error: null, pin_set: false, addresses: [], clients: 0 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

describe("TopBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const fn of Object.values(api)) (fn as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    useStore.setState({ state: base, toasts: [] });
  });

  it("has a Buy me a coffee button that opens the support page", () => {
    render(<TopBar />);
    fireEvent.click(screen.getByText("Buy me a coffee"));
    expect(api.openSupportPage).toHaveBeenCalled();
  });

  it("shows the master volume as a percent", () => {
    render(<TopBar />);
    expect(screen.getByText("80%")).toBeInTheDocument();
  });

  it("follows the slider draft while dragging and commits on release", () => {
    render(<TopBar />);
    const slider = screen.getByLabelText("Master volume");
    fireEvent.change(slider, { target: { value: "0.45" } });
    expect(screen.getByText("45%")).toBeInTheDocument();
    expect(api.setMasterVolume).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    expect(api.setMasterVolume).toHaveBeenCalledWith(0.45);
  });
});
