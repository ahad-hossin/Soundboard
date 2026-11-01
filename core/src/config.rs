//! Persistent configuration: `config.json` in the app data directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_PORT: u16 = 7331;
pub const DEFAULT_STOP_ALL_HOTKEY: &str = "Ctrl+Shift+F12";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    pub id: String,
    pub name: String,
    /// File name relative to `<app_data>/sounds/`.
    pub file: String,
    /// File name relative to `<app_data>/images/`, or None.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default = "one")]
    pub volume: f32,
    #[serde(default)]
    pub hotkey: Option<String>,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Devices {
    #[serde(default)]
    pub speakers: Option<String>,
    #[serde(default)]
    pub virtual_mic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub pin_hash: Option<String>,
    #[serde(default = "new_secret")]
    pub token_secret: String,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self { port: DEFAULT_PORT, pin_hash: None, token_secret: new_secret() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_master")]
    pub master_volume: f32,
    #[serde(default)]
    pub devices: Devices,
    #[serde(default)]
    pub remote: RemoteConfig,
    #[serde(default = "default_stop_all")]
    pub stop_all_hotkey: String,
    #[serde(default)]
    pub sounds: Vec<Sound>,
}

fn default_stop_all() -> String {
    DEFAULT_STOP_ALL_HOTKEY.to_string()
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

fn default_master() -> f32 {
    0.8
}

/// Clamps to 0..=1; a non-finite value (hand-edited file) becomes `fallback`.
fn unit(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            master_volume: default_master(),
            devices: Devices::default(),
            remote: RemoteConfig::default(),
            stop_all_hotkey: default_stop_all(),
            sounds: Vec::new(),
        }
    }
}

impl Config {
    pub fn sound(&self, id: &str) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.id == id)
    }

    pub fn sound_mut(&mut self, id: &str) -> Option<&mut Sound> {
        self.sounds.iter_mut().find(|s| s.id == id)
    }

    /// Repairs values a hand-edited or newer file may carry.
    fn sanitize(mut self) -> Self {
        if self.version > CONFIG_VERSION {
            log::warn!("config version {} is newer than this app ({CONFIG_VERSION}); unknown fields are ignored", self.version);
        }
        self.master_volume = unit(self.master_volume, default_master());
        for s in &mut self.sounds {
            s.volume = unit(s.volume, 1.0);
        }
        self
    }
}

/// 32 random bytes, standard base64 (44 chars).
pub fn new_secret() -> String {
    use base64::Engine;
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[derive(Debug)]
pub enum LoadOutcome {
    Loaded(Config),
    Fresh(Config),
    Recovered { config: Config, backup: PathBuf },
}

impl LoadOutcome {
    pub fn into_config(self) -> Config {
        match self {
            LoadOutcome::Loaded(c) | LoadOutcome::Fresh(c) => c,
            LoadOutcome::Recovered { config, .. } => config,
        }
    }
}

pub fn load(path: &Path) -> LoadOutcome {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return LoadOutcome::Fresh(Config::default()),
        Err(e) => {
            log::warn!("config read failed, backing up and starting fresh: {e}");
            return recover(path);
        }
    };
    match serde_json::from_slice::<Config>(&bytes) {
        Ok(cfg) => LoadOutcome::Loaded(cfg.sanitize()),
        Err(e) => {
            log::warn!("config parse failed, backing up and starting fresh: {e}");
            recover(path)
        }
    }
}

/// Copy the existing config aside before defaulting, so a later save never
/// silently overwrites the user's original file.
fn recover(path: &Path) -> LoadOutcome {
    let backup = path.with_extension("json.bak");
    if let Err(e) = fs::copy(path, &backup) {
        log::warn!("could not back up unreadable config: {e}");
    }
    LoadOutcome::Recovered { config: Config::default(), backup }
}

/// Atomic save: write to `config.json.tmp` then rename over the target.
pub fn save(path: &Path, cfg: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(cfg).map_err(io::Error::other)?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_has_expected_values() {
        let c = Config::default();
        assert_eq!(c.version, CONFIG_VERSION);
        assert_eq!(c.master_volume, 0.8);
        assert_eq!(c.remote.port, DEFAULT_PORT);
        assert_eq!(c.remote.pin_hash, None);
        assert_eq!(c.stop_all_hotkey, DEFAULT_STOP_ALL_HOTKEY);
        assert!(c.sounds.is_empty());
        assert_eq!(c.remote.token_secret.len(), 44, "32 random bytes base64");
    }

    #[test]
    fn missing_file_is_fresh() {
        let dir = tempdir().unwrap();
        match load(&dir.path().join("config.json")) {
            LoadOutcome::Fresh(_) => {}
            other => panic!("expected Fresh, got {other:?}"),
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = Config::default();
        cfg.sounds.push(Sound {
            id: "a".into(),
            name: "Air".into(),
            file: "air.mp3".into(),
            image: Some("a.png".into()),
            volume: 0.5,
            hotkey: Some("Ctrl+Shift+Digit1".into()),
        });
        save(&path, &cfg).unwrap();
        match load(&path) {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded, cfg),
            other => panic!("expected Loaded, got {other:?}"),
        }
        assert!(!path.with_extension("json.tmp").exists(), "temp file cleaned up");
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaulted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not json").unwrap();
        match load(&path) {
            LoadOutcome::Recovered { config, backup } => {
                assert!(config.sounds.is_empty());
                assert_eq!(fs::read_to_string(backup).unwrap(), "{ not json");
            }
            other => panic!("expected Recovered, got {other:?}"),
        }
    }

    #[test]
    fn unreadable_utf8_is_backed_up_and_defaulted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let original = [0xFF, 0xFE, b'{', b'}'];
        fs::write(&path, original).unwrap();
        match load(&path) {
            LoadOutcome::Recovered { config, backup } => {
                assert!(config.sounds.is_empty());
                assert_eq!(fs::read(&backup).unwrap(), original);
            }
            other => panic!("expected Recovered, got {other:?}"),
        }
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn sound_missing_optional_fields_gets_defaults() {
        let json = r#"{"version":1,"master_volume":1.0,"sounds":[{"id":"x","name":"X","file":"x.wav"}]}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.sounds[0].volume, 1.0);
        assert_eq!(cfg.sounds[0].hotkey, None);
        assert_eq!(cfg.sounds[0].image, None);
        assert_eq!(cfg.remote.port, DEFAULT_PORT);
        assert_eq!(cfg.stop_all_hotkey, DEFAULT_STOP_ALL_HOTKEY);
    }

    #[test]
    fn missing_top_level_fields_load_with_defaults() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"remote":{"pin_hash":null},"sounds":[]}"#).unwrap();
        match load(&path) {
            LoadOutcome::Loaded(c) => {
                assert_eq!(c.version, CONFIG_VERSION);
                assert_eq!(c.master_volume, 0.8);
                assert_eq!(c.remote.port, DEFAULT_PORT);
                assert_eq!(c.remote.token_secret.len(), 44);
            }
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn out_of_range_volumes_are_clamped_at_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let json = r#"{"version":2,"master_volume":3.5,"sounds":[
            {"id":"a","name":"A","file":"a.wav","volume":-1},
            {"id":"b","name":"B","file":"b.wav","volume":1.7}]}"#;
        fs::write(&path, json).unwrap();
        let c = load(&path).into_config();
        assert_eq!(c.version, 2, "newer version is kept, only warned about");
        assert_eq!(c.master_volume, 1.0);
        assert_eq!(c.sounds[0].volume, 0.0);
        assert_eq!(c.sounds[1].volume, 1.0);
    }

    #[test]
    fn sound_lookup_by_id() {
        let mut cfg = Config::default();
        cfg.sounds.push(Sound { id: "a".into(), name: "A".into(), file: "a.wav".into(), image: None, volume: 1.0, hotkey: None });
        assert_eq!(cfg.sound("a").unwrap().name, "A");
        assert!(cfg.sound("zzz").is_none());
        cfg.sound_mut("a").unwrap().name = "B".into();
        assert_eq!(cfg.sound("a").unwrap().name, "B");
    }
}
