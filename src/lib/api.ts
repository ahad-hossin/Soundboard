import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface DeviceInfo { name: string; is_default: boolean }
/** Play range in seconds from the start of the file; `end_secs` is exclusive. */
export interface Trim { start_secs: number; end_secs: number }
export interface Waveform {
  /** Length of the whole file, ignoring any trim. */
  duration_secs: number;
  /** Peak per bar across the whole file, 0..1. */
  peaks: number[];
  trim: Trim | null;
}
export interface AddOutcome { rejected: string[]; added_ids: string[] }
export interface UiSound {
  id: string;
  name: string;
  volume: number;
  hotkey: string | null;
  color: string;
  image_path: string | null;
  image_rev: number;
  missing_file: boolean;
  /** Length of what plays (the trimmed range when trimmed) once decoded; null until then. */
  duration_secs: number | null;
  /** Bumped on every successful play; restarts the progress sweep. */
  play_seq: number;
  trim: Trim | null;
}
export interface UiDevices {
  speakers: string | null;
  virtual_mic: string | null;
  microphone: string | null;
  mic_gain: number;
  available: DeviceInfo[];
  available_inputs: DeviceInfo[];
}
export interface UiStartup { autostart: boolean; start_minimized: boolean; close_to_tray: boolean }
export interface UiRemote { port: number | null; error: string | null; pin_set: boolean; addresses: string[]; clients: number }
export interface UiState {
  master_volume: number;
  devices: UiDevices;
  stop_all_hotkey: string;
  startup: UiStartup;
  remote: UiRemote;
  sounds: UiSound[];
  playing: string[];
  startup_notices: string[];
}
export type FirewallState = "allowed" | "blocked" | "none" | "unknown";
export interface FirewallStatus { exe: string; state: FirewallState }
export type ToastLevel = "info" | "warn" | "error";
export interface Toast { level: ToastLevel; message: string }

export const api = {
  getState: () => invoke<UiState>("get_state"),
  play: (id: string) => invoke<void>("play", { id }),
  stopAll: () => invoke<void>("stop_all"),
  setMasterVolume: (value: number) => invoke<void>("set_master_volume", { value }),
  setSoundVolume: (id: string, value: number) => invoke<void>("set_sound_volume", { id, value }),
  addSounds: (paths: string[]) => invoke<AddOutcome>("add_sounds", { paths }),
  getWaveform: (id: string) => invoke<Waveform>("get_waveform", { id }),
  setTrim: (id: string, trim: Trim | null) => invoke<void>("set_trim", { id, trim }),
  previewTrim: (id: string, startSecs: number, endSecs: number) => invoke<void>("preview_trim", { id, startSecs, endSecs }),
  stopPreview: () => invoke<void>("stop_preview"),
  setSoundColor: (id: string, color: string) => invoke<void>("set_sound_color", { id, color }),
  renameSound: (id: string, name: string) => invoke<void>("rename_sound", { id, name }),
  deleteSound: (id: string) => invoke<void>("delete_sound", { id }),
  setSoundImage: (id: string, path: string) => invoke<void>("set_sound_image", { id, path }),
  clearSoundImage: (id: string) => invoke<void>("clear_sound_image", { id }),
  reorderSounds: (ids: string[]) => invoke<void>("reorder_sounds", { ids }),
  setHotkey: (id: string, accelerator: string) => invoke<void>("set_hotkey", { id, accelerator }),
  clearHotkey: (id: string) => invoke<void>("clear_hotkey", { id }),
  setStopAllHotkey: (accelerator: string) => invoke<void>("set_stop_all_hotkey", { accelerator }),
  setDevices: (speakers: string | null, virtualMic: string | null, microphone: string | null) =>
    invoke<void>("set_devices", { speakers, virtualMic, microphone }),
  setMicGain: (value: number) => invoke<void>("set_mic_gain", { value }),
  refreshDevices: () => invoke<void>("refresh_devices"),
  testTone: (device: string | null) => invoke<void>("test_tone", { device }),
  setRemotePort: (port: number) => invoke<void>("set_remote_port", { port }),
  setPin: (pin: string | null) => invoke<void>("set_pin", { pin }),
  setStartup: (autostart: boolean, startMinimized: boolean, closeToTray: boolean) =>
    invoke<void>("set_startup", { autostart, startMinimized, closeToTray }),
  restartRemote: () => invoke<void>("restart_remote"),
  openSupportPage: () => invoke<void>("open_support_page"),
  firewallStatus: () => invoke<FirewallStatus>("firewall_status"),
  firewallAllow: () => invoke<FirewallStatus>("firewall_allow"),
  onState: (cb: (s: UiState) => void): Promise<UnlistenFn> => listen<UiState>("state-changed", (e) => cb(e.payload)),
  onToast: (cb: (t: Toast) => void): Promise<UnlistenFn> => listen<Toast>("toast", (e) => cb(e.payload)),
};
