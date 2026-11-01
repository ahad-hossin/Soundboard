import { pickAndAddSounds } from "../lib/actions";
import { displayAccel } from "../lib/accel";
import { api } from "../lib/api";
import { cx } from "../lib/cx";
import { useStore } from "../lib/store";
import { useSliderDraft } from "../lib/useSliderDraft";

function SpeakerGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M2 6h3l4-3v10l-4-3H2z" />
      <path d="M11 5.5a3 3 0 0 1 0 5" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  );
}

export function TopBar() {
  const state = useStore((s) => s.state)!;
  const run = useStore((s) => s.run);
  const panel = useStore((s) => s.panel);
  const togglePanel = useStore((s) => s.togglePanel);
  const playing = state.playing.length > 0;
  const master = useSliderDraft(state.master_volume, (v) => void run(api.setMasterVolume(v)));

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
      <span className="text-dim">Soundboard</span>
      <span className="text-mute tabular-nums">{state.sounds.length}</span>
      <div className="flex-1" />

      <label className="flex items-center gap-2 text-dim" title="Master volume">
        <SpeakerGlyph />
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          {...master}
          aria-label="Master volume"
          className="w-[120px]"
        />
      </label>

      <button
        type="button"
        onClick={() => run(api.stopAll())}
        title={`Stop all (${displayAccel(state.stop_all_hotkey)})`}
        className={cx("btn", playing ? "border-danger bg-danger text-white hover:bg-danger" : "border-danger text-danger")}
      >
        Stop
      </button>

      <button type="button" onClick={() => void pickAndAddSounds()} className="btn border-text bg-text text-bg hover:bg-text">
        Add
      </button>

      <button type="button" onClick={() => togglePanel("remote")} aria-pressed={panel === "remote"} className="btn relative">
        Remote
        {state.remote.clients > 0 && <span className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-accent" aria-label="Phone connected" />}
      </button>

      <button type="button" onClick={() => togglePanel("settings")} aria-pressed={panel === "settings"} className="btn">
        Settings
      </button>
    </header>
  );
}
