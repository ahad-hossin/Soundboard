import { useEffect, useState } from "react";
import { QRCodeSVG } from "qrcode.react";
import { api, type FirewallState, type FirewallStatus } from "../lib/api";
import { useStore } from "../lib/store";

const FIREWALL_LABEL: Record<FirewallState, string> = {
  allowed: "Windows Firewall: allowed",
  blocked: "Windows Firewall: blocked",
  none: "Windows Firewall: no rule yet",
  unknown: "Windows Firewall: could not check",
};
const FIREWALL_TONE: Record<FirewallState, string> = {
  allowed: "text-dim",
  blocked: "text-danger",
  none: "text-dim",
  unknown: "text-mute",
};

export function RemotePanel() {
  const remote = useStore((s) => s.state!.remote);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  // null while the check runs.
  const [firewall, setFirewall] = useState<FirewallStatus | null>(null);
  const [allowing, setAllowing] = useState(false);
  const running = remote.port !== null;

  // Checks when the panel opens and whenever the server comes up.
  useEffect(() => {
    if (!running) return;
    let live = true;
    setFirewall(null);
    api.firewallStatus().then(
      (s) => live && setFirewall(s),
      () => live && setFirewall({ exe: "", state: "unknown" }),
    );
    return () => { live = false; };
  }, [running]);

  const allow = async () => {
    setAllowing(true);
    try {
      setFirewall(await api.firewallAllow());
    } catch {
      toast("error", "Could not change the firewall rule");
    } finally {
      setAllowing(false);
    }
  };

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
        <div className="mb-4 inline-block overflow-hidden rounded-md">
          {/* Dark modules on a light quiet zone: inverted codes fail on some phone scanners. */}
          <QRCodeSVG value={urls[0]} size={176} bgColor="#ededef" fgColor="#0a0a0b" marginSize={2} level="M" />
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
      <p className="text-dim">Listening on port {remote.port}</p>
      <p className={firewall ? FIREWALL_TONE[firewall.state] : "text-dim"}>
        {firewall ? FIREWALL_LABEL[firewall.state] : "Checking…"}
      </p>
      {firewall && (firewall.state === "blocked" || firewall.state === "none") && (
        <button type="button" className="btn mt-2" disabled={allowing} onClick={() => void allow()}>
          Allow through Windows Firewall
        </button>
      )}
      <p className="mt-4 text-xs text-mute">
        If the phone still cannot connect, check that the PC and phone are on the same Wi-Fi.
      </p>
    </aside>
  );
}
