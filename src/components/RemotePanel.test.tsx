import { render, screen } from "@testing-library/react";
import { useStore } from "../lib/store";
import { RemotePanel } from "./RemotePanel";

vi.mock("../lib/api", () => ({ api: {} }));

const base = {
  master_volume: 0.8,
  devices: { speakers: null, virtual_mic: null, available: [] },
  stop_all_hotkey: "Ctrl+Shift+F12",
  remote: { port: 7331, error: null, pin_set: false, addresses: ["192.168.1.20", "10.0.0.5"], clients: 2 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

describe("RemotePanel", () => {
  it("shows every URL, a QR code and the client count", () => {
    useStore.setState({ state: base });
    render(<RemotePanel />);
    expect(screen.getByText("http://192.168.1.20:7331")).toBeInTheDocument();
    expect(screen.getByText("http://10.0.0.5:7331")).toBeInTheDocument();
    expect(document.querySelector("svg")).toBeInTheDocument();
    expect(screen.getByText(/2 phones connected/)).toBeInTheDocument();
  });

  it("shows the error and Retry when the server is down", () => {
    useStore.setState({ state: { ...base, remote: { ...base.remote, port: null, error: "Could not open port 7331 or the next ten: in use" } } });
    render(<RemotePanel />);
    expect(screen.getByText(/Server not running/)).toBeInTheDocument();
    expect(screen.getByText(/Could not open port/)).toBeInTheDocument();
    expect(screen.getByText("Retry")).toBeInTheDocument();
  });
});
