import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useStore } from "../lib/store";
import { api } from "../lib/api";
import { SettingsPanel } from "./SettingsPanel";

vi.mock("../lib/api", () => ({
  api: {
    refreshDevices: vi.fn(),
    openSupportPage: vi.fn().mockResolvedValue(undefined),
    setDevices: vi.fn(),
    setMicGain: vi.fn(),
    testTone: vi.fn(),
    setStopAllHotkey: vi.fn(),
    setRemotePort: vi.fn(),
    setPin: vi.fn(),
    setStartup: vi.fn(),
  },
}));

const base = {
  master_volume: 0.8,
  devices: {
    speakers: "Ghost Speakers",
    virtual_mic: null,
    microphone: null,
    mic_gain: 1,
    available: [{ name: "Speakers (Realtek)", is_default: true }, { name: "CABLE Input", is_default: false }],
    available_inputs: [{ name: "Headset Mic", is_default: true }, { name: "Webcam Mic", is_default: false }],
  },
  stop_all_hotkey: "Ctrl+Shift+F12",
  startup: { autostart: false, start_minimized: false, close_to_tray: true },
  remote: { port: 7331, error: null, pin_set: false, addresses: ["192.168.1.20"], clients: 0 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

describe("SettingsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const fn of Object.values(api)) (fn as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    useStore.setState({ state: base, toasts: [] });
  });

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

  it("has a Buy me a coffee button that opens the support page", () => {
    render(<SettingsPanel />);
    fireEvent.click(screen.getByText("Buy me a coffee"));
    expect(api.openSupportPage).toHaveBeenCalled();
  });

  it("offers Remove when a PIN is set", () => {
    useStore.setState({ state: { ...base, remote: { ...base.remote, pin_set: true } } });
    render(<SettingsPanel />);
    expect(screen.getByText("Remove")).toBeInTheDocument();
  });

  it("disables Test for an empty slot and enables it for a configured one", () => {
    render(<SettingsPanel />);
    const tests = screen.getAllByRole("button", { name: "Test" });
    expect(tests).toHaveLength(2);
    expect(tests[0]).toBeEnabled();
    expect(tests[1]).toBeDisabled();
  });

  it("passes the other device slots through unchanged when one changes", async () => {
    const user = userEvent.setup();
    useStore.setState({ state: { ...base, devices: { ...base.devices, microphone: "Webcam Mic" } } });
    render(<SettingsPanel />);
    await user.selectOptions(screen.getByLabelText("Speakers"), "CABLE Input");
    expect(api.setDevices).toHaveBeenLastCalledWith("CABLE Input", null, "Webcam Mic");
    await user.selectOptions(screen.getByLabelText("Virtual mic"), "CABLE Input");
    expect(api.setDevices).toHaveBeenLastCalledWith("Ghost Speakers", "CABLE Input", "Webcam Mic");
  });

  it("lists input devices under Microphone and sets the chosen one", async () => {
    const user = userEvent.setup();
    useStore.setState({ state: { ...base, devices: { ...base.devices, virtual_mic: "CABLE Input" } } });
    render(<SettingsPanel />);
    const mic = screen.getByLabelText("Microphone") as HTMLSelectElement;
    expect(mic.value).toBe("");
    expect(within(mic).getByText("None")).toBeInTheDocument();
    expect(within(mic).getByText("Headset Mic (default)")).toBeInTheDocument();
    expect(within(mic).getByText("Webcam Mic")).toBeInTheDocument();
    expect(within(mic).queryByText("CABLE Input")).not.toBeInTheDocument();
    await user.selectOptions(mic, "Headset Mic");
    expect(api.setDevices).toHaveBeenLastCalledWith("Ghost Speakers", "CABLE Input", "Headset Mic");
  });

  it("keeps a missing configured microphone selectable", () => {
    useStore.setState({ state: { ...base, devices: { ...base.devices, microphone: "Old USB Mic" } } });
    render(<SettingsPanel />);
    const mic = screen.getByLabelText("Microphone") as HTMLSelectElement;
    expect(mic.value).toBe("Old USB Mic");
    expect(within(mic).getByText("Old USB Mic (missing)")).toBeInTheDocument();
  });

  it("shows the default mic gain as 100%", () => {
    render(<SettingsPanel />);
    expect(screen.getByText("100%")).toBeInTheDocument();
  });

  it("shows the mic gain and commits it on pointer up", () => {
    useStore.setState({ state: { ...base, devices: { ...base.devices, mic_gain: 1.25 } } });
    render(<SettingsPanel />);
    const gain = screen.getByLabelText("Mic gain") as HTMLInputElement;
    expect(gain.min).toBe("0");
    expect(gain.max).toBe("2");
    expect(gain.step).toBe("0.05");
    expect(screen.getByText("125%")).toBeInTheDocument();
    fireEvent.change(gain, { target: { value: "1.5" } });
    expect(screen.getByText("150%")).toBeInTheDocument();
    expect(api.setMicGain).not.toHaveBeenCalled();
    fireEvent.pointerUp(gain);
    expect(api.setMicGain).toHaveBeenCalledWith(1.5);
  });

  it("rejects an invalid port with a toast and applies a valid one on Enter", async () => {
    const user = userEvent.setup();
    render(<SettingsPanel />);
    const port = screen.getByLabelText("Remote port");
    await user.clear(port);
    await user.type(port, "80");
    await user.click(screen.getByRole("button", { name: "Apply" }));
    expect(useStore.getState().toasts[0].message).toMatch(/1024/);
    expect(api.setRemotePort).not.toHaveBeenCalled();
    await user.clear(port);
    await user.type(port, "8000{Enter}");
    expect(api.setRemotePort).toHaveBeenCalledWith(8000);
  });

  it("filters PIN input, validates length, and sets a valid PIN", async () => {
    const user = userEvent.setup();
    render(<SettingsPanel />);
    const pin = screen.getByLabelText("PIN") as HTMLInputElement;
    await user.type(pin, "12a4");
    expect(pin.value).toBe("124");
    await user.clear(pin);
    await user.type(pin, "123");
    await user.click(screen.getByRole("button", { name: "Set" }));
    expect(useStore.getState().toasts[0].message).toMatch(/4 to 8 digits/);
    expect(api.setPin).not.toHaveBeenCalled();
    await user.clear(pin);
    await user.type(pin, "1234");
    await user.click(screen.getByRole("button", { name: "Set" }));
    expect(api.setPin).toHaveBeenCalledWith("1234");
    expect(pin.value).toBe("");
  });

  it("removes the PIN", async () => {
    const user = userEvent.setup();
    useStore.setState({ state: { ...base, remote: { ...base.remote, pin_set: true } } });
    render(<SettingsPanel />);
    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(api.setPin).toHaveBeenCalledWith(null);
  });

  it("renders the three Startup checkboxes with the saved values", () => {
    useStore.setState({ state: { ...base, startup: { autostart: true, start_minimized: false, close_to_tray: true } } });
    render(<SettingsPanel />);
    expect(screen.getByLabelText("Start with Windows")).toBeChecked();
    expect(screen.getByLabelText("Start minimized to tray")).not.toBeChecked();
    expect(screen.getByLabelText("Close to tray")).toBeChecked();
  });

  it("changes one Startup option and passes the other two through", async () => {
    const user = userEvent.setup();
    render(<SettingsPanel />);
    await user.click(screen.getByLabelText("Start with Windows"));
    expect(api.setStartup).toHaveBeenLastCalledWith(true, false, true);
    await user.click(screen.getByLabelText("Start minimized to tray"));
    expect(api.setStartup).toHaveBeenLastCalledWith(false, true, true);
    await user.click(screen.getByLabelText("Close to tray"));
    expect(api.setStartup).toHaveBeenLastCalledWith(false, false, false);
  });

  it("opens the hotkey capture dialog from Change", async () => {
    const user = userEvent.setup();
    render(<SettingsPanel />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Change" }));
    expect(screen.getByRole("dialog")).toBeVisible();
  });
});
