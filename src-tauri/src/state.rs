use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use soundboard_core::audio::{DeviceInfo, Engine, Outputs, RodioBackend};
use soundboard_core::config::{self, Config};
use soundboard_core::library::Library;
use soundboard_core::remote::protocol::{RemoteSound, ServerMsg};
use soundboard_core::remote::Hub;
use tauri::{AppHandle, Emitter};

pub const CACHE_BYTES: usize = 200 * 1024 * 1024;

pub struct App {
    pub config: Config,
    pub engine: Engine<RodioBackend>,
    pub library: Library,
    pub remote_port: Option<u16>,
    pub remote_error: Option<String>,
    /// Device names already reported as missing this session.
    pub warned_devices: HashSet<String>,
    /// Shown once as toasts when the UI first loads.
    pub startup_notices: Vec<String>,
}

pub type Shared = Arc<Mutex<App>>;

/// Managed Tauri state.
pub struct Ctx {
    pub shared: Shared,
    pub hub: Hub,
    pub remote: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
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
    pub hotkey: Option<String>,
    pub image_path: Option<String>,
    pub missing_file: bool,
}

#[derive(Serialize, Clone)]
pub struct UiDevices {
    pub speakers: Option<String>,
    pub virtual_mic: Option<String>,
    pub available: Vec<DeviceInfo>,
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
    pub remote: UiRemote,
    pub sounds: Vec<UiSound>,
    pub playing: Vec<String>,
    pub startup_notices: Vec<String>,
}

impl App {
    pub fn outputs(&self) -> Outputs {
        Outputs { speakers: self.config.devices.speakers.clone(), virtual_mic: self.config.devices.virtual_mic.clone() }
    }

    pub fn save(&self) {
        if let Err(e) = config::save(&self.library.config_path(), &self.config) {
            log::error!("config save failed: {e}");
        }
    }

    /// Plays a sound; returns toasts for device fallbacks. Error string is user-facing.
    pub fn play(&mut self, id: &str) -> Result<Vec<Toast>, String> {
        let sound = self.config.sound(id).cloned().ok_or_else(|| "That sound no longer exists".to_string())?;
        let path = self.library.sound_path(&sound);
        if !path.exists() {
            return Err(format!("The file for \"{}\" is missing", sound.name));
        }
        let outcome = self
            .engine
            .play_file(&sound.id, sound.volume, &path)
            .map_err(|e| format!("Could not play \"{}\": {e}", sound.name))?;
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
                available: self.engine.devices(),
            },
            stop_all_hotkey: self.config.stop_all_hotkey.clone(),
            remote: UiRemote {
                port: self.remote_port,
                error: self.remote_error.clone(),
                pin_set: self.config.remote.pin_hash.is_some(),
                addresses: lan_addresses(),
                clients: hub.client_count(),
            },
            sounds: self
                .config
                .sounds
                .iter()
                .map(|s| UiSound {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    volume: s.volume,
                    hotkey: s.hotkey.clone(),
                    image_path: self.library.image_path(s).map(|p| p.to_string_lossy().into_owned()),
                    missing_file: !self.library.sound_path(s).exists(),
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
                .map(|s| RemoteSound { id: s.id.clone(), name: s.name.clone(), volume: s.volume, has_image: s.image.is_some() })
                .collect(),
        }
    }
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
