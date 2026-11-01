import { useEffect, useState } from "react";
import { displayAccel } from "../lib/accel";
import { api, type DeviceInfo } from "../lib/api";
import { useStore } from "../lib/store";
import { validatePin, validatePort } from "../lib/validate";
import { HotkeyCapture } from "./HotkeyCapture";

function DeviceSelect({ id, label, value, available, onChange, onTest }: {
  id: string; label: string; value: string | null; available: DeviceInfo[];
  onChange: (v: string | null) => void; onTest: () => void;
}) {
  const missing = value && !available.some((d) => d.name === value);
  return (
    <div className="mb-4">
      <label className="label" htmlFor={id}>{label}</label>
      <div className="flex gap-2">
        <select id={id} className="field" value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
          <option value="">None</option>
          {missing && <option value={value}>{value} (missing)</option>}
          {available.map((d) => (
            <option key={d.name} value={d.name}>{d.name}{d.is_default ? " (default)" : ""}</option>
          ))}
        </select>
        <button type="button" className="btn" disabled={!value} onClick={onTest}>Test</button>
      </div>
    </div>
  );
}

export function SettingsPanel() {
  const state = useStore((s) => s.state)!;
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [capturing, setCapturing] = useState(false);
  const [port, setPort] = useState(String(state.remote.port ?? 7331));
  const [pin, setPin] = useState("");
  const { speakers, virtual_mic, available } = state.devices;

  // The device list is cached in the backend; re-enumerate whenever the panel opens.
  useEffect(() => {
    void run(api.refreshDevices());
  }, [run]);

  const applyPort = () => {
    const err = validatePort(port);
    if (err) { toast("error", err); return; }
    void run(api.setRemotePort(Number(port)));
  };

  const applyPin = () => {
    const err = validatePin(pin);
    if (err) { toast("error", err); return; }
    void run(api.setPin(pin));
    setPin("");
  };

  return (
    <aside className="panel">
      <DeviceSelect id="dev-speakers" label="Speakers" value={speakers} available={available}
        onChange={(v) => void run(api.setDevices(v, virtual_mic))} onTest={() => void run(api.testTone(speakers))} />
      <DeviceSelect id="dev-mic" label="Virtual mic" value={virtual_mic} available={available}
        onChange={(v) => void run(api.setDevices(speakers, v))} onTest={() => void run(api.testTone(virtual_mic))} />

      <div className="mb-4">
        <span className="label">Stop All hotkey</span>
        <div className="flex items-center gap-2">
          <span className="font-mono">{displayAccel(state.stop_all_hotkey)}</span>
          <button type="button" className="btn ml-auto" onClick={() => setCapturing(true)}>Change</button>
        </div>
      </div>

      <div className="mb-4">
        <label className="label" htmlFor="port">Remote port</label>
        <div className="flex gap-2">
          <input id="port" className="field" inputMode="numeric" value={port} onChange={(e) => setPort(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") applyPort(); }} />
          <button type="button" className="btn" onClick={applyPort} disabled={Number(port) === state.remote.port}>Apply</button>
        </div>
      </div>

      <div className="mb-2">
        <label className="label" htmlFor="pin">PIN</label>
        {state.remote.pin_set ? (
          <div className="flex items-center gap-2">
            <span className="text-dim">A PIN is set</span>
            <button type="button" className="btn ml-auto border-danger text-danger" onClick={() => void run(api.setPin(null))}>Remove</button>
          </div>
        ) : (
          <div className="flex gap-2">
            <input id="pin" className="field" inputMode="numeric" placeholder="4 to 8 digits" value={pin}
              onChange={(e) => setPin(e.target.value.replace(/\D/g, "").slice(0, 8))}
              onKeyDown={(e) => { if (e.key === "Enter") applyPin(); }} />
            <button type="button" className="btn" onClick={applyPin} disabled={!pin}>Set</button>
          </div>
        )}
      </div>
      <p className="text-xs text-mute">
        The remote is for your local network only. Traffic is unencrypted. The PIN stops other people on your
        Wi-Fi from triggering sounds; it is not protection against a hostile network.
      </p>

      {capturing && (
        <HotkeyCapture
          title="Stop All hotkey"
          current={state.stop_all_hotkey}
          onCancel={() => setCapturing(false)}
          onCapture={(accel) => { void run(api.setStopAllHotkey(accel)); setCapturing(false); }}
        />
      )}
    </aside>
  );
}
