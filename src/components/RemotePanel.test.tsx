import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useStore } from "../lib/store";
import { api } from "../lib/api";
import { RemotePanel } from "./RemotePanel";

vi.mock("../lib/api", () => ({ api: { restartRemote: vi.fn() } }));

const base = {
  master_volume: 0.8,
  devices: { speakers: null, virtual_mic: null, available: [] },
  stop_all_hotkey: "Ctrl+Shift+F12",
  remote: { port: 7331, error: null, pin_set: false, addresses: ["192.168.1.20", "10.0.0.5"], clients: 2 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

const down = { ...base, remote: { ...base.remote, port: null, error: "Could not open port 7331 or the next ten: in use" } };

describe("RemotePanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.restartRemote).mockResolvedValue(undefined);
    useStore.setState({ state: base, toasts: [] });
  });

  it("shows every URL, a QR code and the client count", () => {
    render(<RemotePanel />);
    expect(screen.getByText("http://192.168.1.20:7331")).toBeInTheDocument();
    expect(screen.getByText("http://10.0.0.5:7331")).toBeInTheDocument();
    const qr = document.querySelector("svg")!;
    expect(qr).toBeInTheDocument();
    // Scanner-friendly polarity: light background, dark modules.
    const fills = [...qr.querySelectorAll("path")].map((p) => p.getAttribute("fill"));
    expect(fills).toEqual(["#ededef", "#0a0a0b"]);
    expect(screen.getByText(/2 phones connected/)).toBeInTheDocument();
  });

  it("shows the error and Retry when the server is down", () => {
    useStore.setState({ state: down });
    render(<RemotePanel />);
    expect(screen.getByText(/Server not running/)).toBeInTheDocument();
    expect(screen.getByText(/Could not open port/)).toBeInTheDocument();
    expect(screen.getByText("Retry")).toBeInTheDocument();
  });

  it("restarts the server on Retry", async () => {
    const user = userEvent.setup();
    useStore.setState({ state: down });
    render(<RemotePanel />);
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(api.restartRemote).toHaveBeenCalledTimes(1);
  });

  it("copies the URL and confirms with a toast", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(navigator, "clipboard", "get").mockReturnValue({ writeText } as unknown as Clipboard);
    render(<RemotePanel />);
    await user.click(screen.getAllByRole("button", { name: "Copy" })[0]);
    expect(writeText).toHaveBeenCalledWith("http://192.168.1.20:7331");
    await vi.waitFor(() => expect(useStore.getState().toasts[0]).toMatchObject({ level: "info", message: "Copied" }));
  });

  it("toasts an error when the clipboard write fails", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    vi.spyOn(navigator, "clipboard", "get").mockReturnValue({ writeText } as unknown as Clipboard);
    render(<RemotePanel />);
    await user.click(screen.getAllByRole("button", { name: "Copy" })[0]);
    await vi.waitFor(() => expect(useStore.getState().toasts[0]).toMatchObject({ level: "error", message: "Could not copy" }));
  });

  it("pluralizes the client count", () => {
    useStore.setState({ state: { ...base, remote: { ...base.remote, clients: 0 } } });
    const { unmount } = render(<RemotePanel />);
    expect(screen.getByText("No phones connected")).toBeInTheDocument();
    unmount();
    useStore.setState({ state: { ...base, remote: { ...base.remote, clients: 1 } } });
    render(<RemotePanel />);
    expect(screen.getByText("1 phone connected")).toBeInTheDocument();
  });
});
