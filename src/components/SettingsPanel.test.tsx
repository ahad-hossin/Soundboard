import { render, screen, within } from "@testing-library/react";
import { useStore } from "../lib/store";
import { api } from "../lib/api";
import { SettingsPanel } from "./SettingsPanel";

vi.mock("../lib/api", () => ({ api: { refreshDevices: vi.fn().mockResolvedValue(undefined) } }));

const base = {
  master_volume: 0.8,
  devices: { speakers: "Ghost Speakers", virtual_mic: null, available: [{ name: "Speakers (Realtek)", is_default: true }, { name: "CABLE Input", is_default: false }] },
  stop_all_hotkey: "Ctrl+Shift+F12",
  remote: { port: 7331, error: null, pin_set: false, addresses: ["192.168.1.20"], clients: 0 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

describe("SettingsPanel", () => {
  beforeEach(() => useStore.setState({ state: base }));

  it("lists devices and keeps a missing configured device selectable", () => {
    render(<SettingsPanel />);
    const speakers = screen.getByLabelText("Speakers") as HTMLSelectElement;
    expect(speakers.value).toBe("Ghost Speakers");
    expect(within(speakers).getByText("Ghost Speakers (missing)")).toBeInTheDocument();
    expect(within(speakers).getByText("Speakers (Realtek) (default)")).toBeInTheDocument();
    expect((screen.getByLabelText("Virtual mic") as HTMLSelectElement).value).toBe("");
  });

  it("refreshes the device list when it opens", () => {
    render(<SettingsPanel />);
    expect(api.refreshDevices).toHaveBeenCalled();
  });

  it("shows the Stop All hotkey and the PIN set form", () => {
    render(<SettingsPanel />);
    expect(screen.getByText("Ctrl+Shift+F12")).toBeInTheDocument();
    expect(screen.getByLabelText("PIN")).toBeInTheDocument();
    expect(screen.getByText(/local network only/i)).toBeInTheDocument();
  });

  it("offers Remove when a PIN is set", () => {
    useStore.setState({ state: { ...base, remote: { ...base.remote, pin_set: true } } });
    render(<SettingsPanel />);
    expect(screen.getByText("Remove")).toBeInTheDocument();
  });
});
