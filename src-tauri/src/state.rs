use std::collections::{HashMap, HashSet};
use std::io;
use std::net::IpAddr;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use soundboard_core::audio::decode::decode_file;
use soundboard_core::audio::{DeviceInfo, Engine, Outputs, RodioBackend};
use soundboard_core::config::{self, Config, Sound, Startup};
use soundboard_core::library::Library;
use soundboard_core::palette::PALETTE;
use soundboard_core::remote::protocol::{RemoteSound, ServerMsg};
use soundboard_core::remote::Hub;
use tauri::{AppHandle, Emitter, Manager};

pub const CACHE_BYTES: usize = 200 * 1024 * 1024;

pub struct App {
    pub config: Config,
    pub engine: Engine<RodioBackend>,
    pub library: Library,
    pub remote_port: Option<u16>,
    pub remote_error: Option<String>,
    /// Device names already reported as missing this session.
    pub warned_devices: HashSet<String>,
    /// Cached output device list; enumerating is slow, so it is refreshed only on demand.
    pub available_devices: Vec<DeviceInfo>,
    /// Cached input device list, refreshed alongside `available_devices`.
    pub available_inputs: Vec<DeviceInfo>,
    /// Last mic passthrough failure shown as a toast, so the same failure is reported once.
    pub last_passthrough_error: Option<String>,
    /// Shown once as toasts when the UI first loads.
    pub startup_notices: Vec<String>,
    /// Config has changes not yet written; the reaper flushes it every tick.
    pub dirty: bool,
    /// A save failure has been shown; reset after the next successful write.
    pub save_failed_reported: bool,
    /// LAN addresses for the Remote panel, refreshed when the server starts or devices refresh.
    pub addresses: Vec<String>,
    /// Per sound id: (file missing, image mtime in seconds). Refreshed by `refresh_files`.
    pub files: HashMap<String, (bool, u64)>,
    /// Per sound id: how many times it has started this session. The UI and phone restart a
    /// tile's progress sweep whenever this changes.
    pub play_seq: HashMap<String, u64>,
}

pub type Shared = Arc<Mutex<App>>;

/// Managed Tauri state.
pub struct Ctx {
    pub shared: Shared,
    pub hub: Hub,
    pub remote: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// True once the tray icon exists. Hiding the window (close to tray, minimized start) is only
    /// allowed then, so a hidden window can always be brought back.
    pub tray_ok: AtomicBool,
}

pub fn lock(shared: &Shared) -> MutexGuard<'_, App> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Serialize, Clone)]
pub struct Toast {
    pub level: String,
    pub message: String,
}

impl Toast {
    pub fn warn(message: impl Into<String>) -> Self {
        Self { level: "warn".into(), message: message.into() }
    }
}

#[derive(Serialize, Clone)]
pub struct UiSound {
    pub id: String,
    pub name: String,
    pub volume: f32,
    pub color: String,
    pub hotkey: Option<String>,
    pub image_path: Option<String>,
    pub image_rev: u64,
    pub missing_file: bool,
    /// Clip length once decoded and cached.
    pub duration_secs: Option<f32>,
    pub play_seq: u64,
}

#[derive(Serialize, Clone)]
pub struct UiDevices {
    pub speakers: Option<String>,
    pub virtual_mic: Option<String>,
    pub microphone: Option<String>,
    pub mic_gain: f32,
    pub available: Vec<DeviceInfo>,
    pub available_inputs: Vec<DeviceInfo>,
}

#[derive(Serialize, Clone)]
pub struct UiRemote {
    pub port: Option<u16>,
    pub error: Option<String>,
    pub pin_set: bool,
    pub addresses: Vec<String>,
    pub clients: usize,
}

#[derive(Serialize, Clone)]
pub struct UiState {
    pub master_volume: f32,
    pub devices: UiDevices,
    pub stop_all_hotkey: String,
    pub startup: Startup,
    pub remote: UiRemote,
    pub sounds: Vec<UiSound>,
    pub playing: Vec<String>,
    pub startup_notices: Vec<String>,
}

impl App {
    pub fn outputs(&self) -> Outputs {
        let d = &self.config.devices;
        Outputs {
            speakers: d.speakers.clone(),
            virtual_mic: d.virtual_mic.clone(),
            microphone: d.microphone.clone(),
            mic_gain: d.mic_gain,
        }
    }

    pub fn new(
        config: Config,
        engine: Engine<RodioBackend>,
        library: Library,
        available_devices: Vec<DeviceInfo>,
        available_inputs: Vec<DeviceInfo>,
    ) -> Self {
        let mut app = Self {
            config,
            engine,
            library,
            remote_port: None,
            remote_error: None,
            warned_devices: HashSet::new(),
            available_devices,
            available_inputs,
            last_passthrough_error: None,
            startup_notices: Vec::new(),
            dirty: false,
            save_failed_reported: false,
            addresses: Vec::new(),
            files: HashMap::new(),
            play_seq: HashMap::new(),
        };
        app.refresh_files();
        app
    }

    /// Re-enumerates output and input devices into the caches.
    pub fn refresh_device_lists(&mut self) {
        self.available_devices = self.engine.devices();
        self.available_inputs = self.engine.input_devices();
    }

    /// Toast text for the engine's last passthrough failure, or None when passthrough
    /// worked or this exact failure was already reported. A success re-arms the toast.
    pub fn passthrough_toast(&mut self) -> Option<String> {
        self.passthrough_toast_with("Microphone passthrough failed")
    }

    /// `passthrough_toast` with a caller-chosen lead-in, formatted as "{prefix}: {error}".
    pub fn passthrough_toast_with(&mut self, prefix: &str) -> Option<String> {
        match self.engine.passthrough_error() {
            None => {
                self.last_passthrough_error = None;
                None
            }
            Some(e) if self.last_passthrough_error.as_deref() == Some(e.as_str()) => None,
            Some(e) => {
                let message = format!("{prefix}: {e}");
                self.last_passthrough_error = Some(e);
                Some(message)
            }
        }
    }

    /// Sets one sound's volume in the config and on any live voices. False if the id is unknown.
    pub fn set_sound_volume(&mut self, id: &str, value: f32) -> bool {
        let Some(s) = self.config.sound_mut(id) else {
            return false;
        };
        let v = value.clamp(0.0, 1.0);
        s.volume = v;
        self.engine.set_sound_volume(id, v);
        self.mark_dirty();
        true
    }

    /// Records that config changed; the reaper writes it within one tick.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Writes config if it changed. On error the flag stays set so the next tick retries.
    pub fn flush(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        config::save(&self.library.config_path(), &self.config)?;
        self.dirty = false;
        // Any successful write (reaper, immediate or on exit) re-arms the failure toast.
        self.save_failed_reported = false;
        Ok(())
    }

    /// Re-stats every sound file and image. Call after import, delete and image changes.
    pub fn refresh_files(&mut self) {
        self.files = self
            .config
            .sounds
            .iter()
            .map(|s| {
                let missing = !self.library.sound_path(s).exists();
                let image_rev = self
                    .library
                    .image_path(s)
                    .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (s.id.clone(), (missing, image_rev))
            })
            .collect();
    }

    /// Plays a sound; returns toasts for device fallbacks. Error string is user-facing.
    pub fn play(&mut self, id: &str) -> Result<Vec<Toast>, String> {
        let sound = self.config.sound(id).cloned().ok_or_else(|| "That sound no longer exists".to_string())?;
        let path = self.library.sound_path(&sound);
        if !path.exists() {
            self.files.entry(sound.id.clone()).or_default().0 = true;
            return Err(format!("The file for \"{}\" is missing", sound.name));
        }
        let outcome = self
            .engine
            .play_file(&sound.id, sound.volume, &path, sound.trim)
            .map_err(|e| format!("Could not play \"{}\": {e}", sound.name))?;
        // The file is evidently there (again); clear a stale missing flag.
        self.files.entry(sound.id.clone()).or_default().0 = false;
        *self.play_seq.entry(sound.id.clone()).or_insert(0) += 1;
        let mut toasts = Vec::new();
        for dev in outcome.fallbacks {
            if self.warned_devices.insert(dev.clone()) {
                toasts.push(Toast::warn(format!("Output device \"{dev}\" not found, using the default device")));
            }
        }
        Ok(toasts)
    }

    pub fn ui_state(&self, hub: &Hub) -> UiState {
        UiState {
            master_volume: self.config.master_volume,
            devices: UiDevices {
                speakers: self.config.devices.speakers.clone(),
                virtual_mic: self.config.devices.virtual_mic.clone(),
                microphone: self.config.devices.microphone.clone(),
                mic_gain: self.config.devices.mic_gain,
                available: self.available_devices.clone(),
                available_inputs: self.available_inputs.clone(),
            },
            stop_all_hotkey: self.config.stop_all_hotkey.clone(),
            startup: self.config.startup.clone(),
            remote: UiRemote {
                port: self.remote_port,
                error: self.remote_error.clone(),
                pin_set: self.config.remote.pin_hash.is_some(),
                addresses: self.addresses.clone(),
                clients: hub.client_count(),
            },
            sounds: self
                .config
                .sounds
                .iter()
                .map(|s| {
                    let (missing_file, image_rev) = self.files.get(&s.id).copied().unwrap_or((false, 0));
                    UiSound {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        volume: s.volume,
                        hotkey: s.hotkey.clone(),
                        color: tile_color(s),
                        image_path: self.library.image_path(s).map(|p| p.to_string_lossy().into_owned()),
                        image_rev,
                        missing_file,
                        duration_secs: self.engine.cached_duration(&s.id, s.trim),
                        play_seq: self.play_seq.get(&s.id).copied().unwrap_or(0),
                    }
                })
                .collect(),
            playing: self.engine.playing(),
            startup_notices: self.startup_notices.clone(),
        }
    }

    pub fn remote_snapshot(&self) -> ServerMsg {
        ServerMsg::State {
            master_volume: self.config.master_volume,
            playing: self.engine.playing(),
            sounds: self
                .config
                .sounds
                .iter()
                .map(|s| RemoteSound {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    volume: s.volume,
                    has_image: s.image.is_some(),
                    color: tile_color(s),
                    duration_secs: self.engine.cached_duration(&s.id, s.trim),
                    play_seq: self.play_seq.get(&s.id).copied().unwrap_or(0),
                })
                .collect(),
        }
    }
}

/// The sound's color; sanitize always sets one, so the fallback only covers a sound built by hand.
fn tile_color(s: &Sound) -> String {
    s.color.clone().unwrap_or_else(|| PALETTE[0].to_string())
}

/// Non-loopback, non-link-local IPv4 addresses of this machine.
pub fn lan_addresses() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .map(|list| {
            list.into_iter()
                .filter_map(|(_, ip)| match ip {
                    IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() => Some(v4.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Plays a sound for a hotkey or phone press and reports the outcome as toasts.
/// Blocking (first play may decode, opening a device is slow): run it in `spawn_blocking`.
pub fn play_and_report(app: &AppHandle, id: &str) {
    let ctx = app.state::<Ctx>();
    let result = lock(&ctx.shared).play(id);
    match result {
        Ok(toasts) => {
            for t in toasts {
                emit_toast(app, &t.level, t.message);
            }
        }
        Err(e) => emit_toast(app, "error", e),
    }
    emit_state(app, &ctx);
}

/// Decodes sounds into the engine cache in the background so the first play is instant.
/// The decode runs outside the App lock; the lock is taken only to look up the path and to
/// insert the result. `None` warms every sound in config order.
pub fn spawn_warm(shared: Shared, ids: Option<Vec<String>>) {
    tauri::async_runtime::spawn_blocking(move || {
        let ids = ids.unwrap_or_else(|| lock(&shared).config.sounds.iter().map(|s| s.id.clone()).collect());
        for id in ids {
            let path = {
                let a = lock(&shared);
                match a.config.sound(&id) {
                    Some(s) if !a.engine.is_cached(&id) => a.library.sound_path(s),
                    _ => continue,
                }
            };
            let buffer = match decode_file(&path) {
                Ok(b) => b,
                Err(e) => {
                    log::debug!("warming {id} failed: {e}");
                    continue;
                }
            };
            {
                let mut a = lock(&shared);
                // The sound may have been deleted or played (and cached) while decoding.
                if a.config.sound(&id).is_some() && !a.engine.is_cached(&id) && !a.engine.insert_cached(&id, buffer) {
                    break; // cache full; warming never evicts
                }
            }
            std::thread::yield_now();
        }
    });
}

/// Pushes current state to the desktop UI and every phone. Never call while holding the App lock.
pub fn emit_state(app: &AppHandle, ctx: &Ctx) {
    let (ui, snap) = {
        let a = lock(&ctx.shared);
        (a.ui_state(&ctx.hub), a.remote_snapshot())
    };
    if let Err(e) = app.emit("state-changed", &ui) {
        log::warn!("emit state failed: {e}");
    }
    ctx.hub.broadcast_state(&snap);
}

pub fn emit_toast(app: &AppHandle, level: &str, message: impl Into<String>) {
    let _ = app.emit("toast", Toast { level: level.into(), message: message.into() });
}
