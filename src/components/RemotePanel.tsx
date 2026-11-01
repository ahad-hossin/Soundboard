import { QRCodeSVG } from "qrcode.react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";

export function RemotePanel() {
  const remote = useStore((s) => s.state!.remote);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);

  if (remote.port === null) {
    return (
      <aside className="panel">
        <p className="mb-1">Server not running.</p>
        {remote.error && <p className="mb-3 text-xs text-dim">{remote.error}</p>}
        <button type="button" className="btn" onClick={() => void run(api.restartRemote())}>Retry</button>
      </aside>
    );
  }

  const urls = remote.addresses.map((ip) => `http://${ip}:${remote.port}`);
  const copy = (u: string) => {
    void navigator.clipboard.writeText(u).then(() => toast("info", "Copied")).catch(() => toast("error", "Could not copy"));
  };

  return (
    <aside className="panel">
      {urls.length === 0 && <p className="mb-3 text-dim">No network connection.</p>}
      {urls[0] && (
        <div className="mb-4 inline-block rounded-md bg-surface p-3">
          <QRCodeSVG value={urls[0]} size={160} bgColor="#141416" fgColor="#ededef" level="M" />
        </div>
      )}
      <ul className="mb-4 space-y-1">
        {urls.map((u) => (
          <li key={u} className="flex items-center gap-2">
            <span className="select-text font-mono text-xs">{u}</span>
            <button type="button" className="btn ml-auto h-7 px-2 text-xs" onClick={() => copy(u)}>Copy</button>
          </li>
        ))}
      </ul>
      <p className="mb-4 text-dim">
        {remote.clients === 0 ? "No phones connected" : remote.clients === 1 ? "1 phone connected" : `${remote.clients} phones connected`}
      </p>
      <p className="text-xs text-mute">
        If your phone cannot connect, allow Soundboard through Windows Firewall on private networks.
      </p>
    </aside>
  );
}
