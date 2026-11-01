//! Persistent configuration: `config.json` in the app data directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::palette::next_color;

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
    /// Tile color as `#RRGGBB`, always set after `Config::sanitize`.
    #[serde(default)]
    pub color: Option<String>,
    /// Non-destructive crop: only this range plays. None plays the whole file.
    #[serde(default)]
    pub trim: Option<Trim>,
}

/// Shortest range a trim may keep, in seconds.
pub const MIN_TRIM_SECS: f32 = 0.05;

/// Play range of a sound in seconds from the start of its file; `end_secs` is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Trim {
    pub start_secs: f32,
    pub end_secs: f32,
}

impl Trim {
    /// Finite, non-negative and at least `MIN_TRIM_SECS` long (with a little slack for f32
    /// rounding, so a range the editor clamped to exactly 50 ms survives a reload). Says nothing
    /// about the file length.
    pub fn is_valid(&self) -> bool {
        self.start_secs.is_finite()
            && self.end_secs.is_finite()
            && self.start_secs >= 0.0
            && self.end_secs - self.start_secs >= MIN_TRIM_SECS - 1e-4
    }
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Devices {
    #[serde(default)]
    pub speakers: Option<String>,
    #[serde(default)]
    pub virtual_mic: Option<String>,
    /// Input device mixed into the virtual mic output, or None for no passthrough.
    #[serde(default)]
    pub microphone: Option<String>,
    /// Linear passthrough gain, 0..=2.
    #[serde(default = "one")]
    pub mic_gain: f32,
}

impl Default for Devices {
    fn default() -> Self {
        Self { speakers: None, virtual_mic: None, microphone: None, mic_gain: 1.0 }
    }
}

pub const MAX_MIC_GAIN: f32 = 2.0;

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

/// Desktop startup behaviour. `autostart` mirrors the Windows Run key entry the app registers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Startup {
    #[serde(default)]
    pub autostart: bool,
    /// Hide the main window at launch; only the tray icon shows.
    #[serde(default)]
    pub start_minimized: bool,
    /// Closing the window hides it to the tray instead of quitting.
    #[serde(default = "yes")]
    pub close_to_tray: bool,
}

fn yes() -> bool {
    true
}

impl Default for Startup {
    fn default() -> Self {
        Self { autostart: false, start_minimized: false, close_to_tray: true }
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
    pub startup: Startup,
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

/// `#RRGGBB` with hex digits of either case.
fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
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
            startup: Startup::default(),
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

    /// Normalizes valid `#RRGGBB` colors to uppercase, then gives every sound without one the
    /// least-used palette color so far, so existing libraries get distinct colors on first load.
    fn assign_colors(&mut self) {
        for s in &mut self.sounds {
            s.color = match s.color.take() {
                Some(c) if is_hex_color(&c) => Some(c.to_ascii_uppercase()),
                _ => None,
            };
        }
        let mut used: Vec<String> = self.sounds.iter().filter_map(|s| s.color.clone()).collect();
        for s in self.sounds.iter_mut().filter(|s| s.color.is_none()) {
            let next = next_color(&used.iter().map(String::as_str).collect::<Vec<_>>());
            used.push(next.to_string());
            s.color = Some(next.to_string());
        }
    }

    /// Repairs values a hand-edited or newer file may carry.
    fn sanitize(mut self) -> Self {
        if self.version > CONFIG_VERSION {
            log::warn!("config version {} is newer than this app ({CONFIG_VERSION}); unknown fields are ignored", self.version);
        }
        self.master_volume = unit(self.master_volume, default_master());
        for s in &mut self.sounds {
            s.volume = unit(s.volume, 1.0);
            s.trim = s.trim.filter(Trim::is_valid);
        }
        self.assign_colors();
        self.devices.mic_gain =
            if self.devices.mic_gain.is_finite() { self.devices.mic_gain.clamp(0.0, MAX_MIC_GAIN) } else { 1.0 };
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
    use crate::palette::PALETTE;

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
        assert_eq!(c.devices.microphone, None);
        assert_eq!(c.devices.mic_gain, 1.0);
        assert_eq!(c.startup, Startup { autostart: false, start_minimized: false, close_to_tray: true });
    }

    #[test]
    fn config_without_startup_key_closes_to_tray() {
        let c = load_json(r#"{"sounds":[]}"#);
        assert!(c.startup.close_to_tray);
        assert!(!c.startup.autostart);
        assert!(!c.startup.start_minimized);
        let partial = load_json(r#"{"startup":{"autostart":true}}"#);
        assert!(partial.startup.autostart);
        assert!(partial.startup.close_to_tray, "missing close_to_tray defaults to on");
    }

    #[test]
    fn startup_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = Config::default();
        cfg.startup = Startup { autostart: true, start_minimized: true, close_to_tray: false };
        save(&path, &cfg).unwrap();
        assert_eq!(load(&path).into_config().startup, cfg.startup);
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
            color: Some("#FF595E".into()),
            trim: Some(Trim { start_secs: 0.25, end_secs: 1.5 }),
        });
        cfg.devices = Devices {
            speakers: Some("Speakers".into()),
            virtual_mic: Some("CABLE Input".into()),
            microphone: Some("Headset Mic".into()),
            mic_gain: 1.5,
        };
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
        assert_eq!(cfg.sounds[0].trim, None);
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
    fn devices_without_mic_keys_load_with_defaults() {
        let json = r#"{"devices":{"speakers":"Speakers","virtual_mic":"CABLE Input"}}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.devices.speakers.as_deref(), Some("Speakers"));
        assert_eq!(cfg.devices.microphone, None);
        assert_eq!(cfg.devices.mic_gain, 1.0);
    }

    #[test]
    fn mic_gain_is_clamped_at_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{"devices":{"microphone":"Mic","mic_gain":3.0}}"#).unwrap();
        let c = load(&path).into_config();
        assert_eq!(c.devices.microphone.as_deref(), Some("Mic"));
        assert_eq!(c.devices.mic_gain, 2.0);
        fs::write(&path, r#"{"devices":{"mic_gain":-0.5}}"#).unwrap();
        assert_eq!(load(&path).into_config().devices.mic_gain, 0.0);
    }

    fn load_json(json: &str) -> Config {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, json).unwrap();
        load(&path).into_config()
    }

    fn sounds_json(colors: &[Option<&str>]) -> String {
        let items: Vec<String> = colors
            .iter()
            .enumerate()
            .map(|(i, c)| match c {
                Some(c) => format!(r#"{{"id":"s{i}","name":"S","file":"s.wav","color":"{c}"}}"#),
                None => format!(r#"{{"id":"s{i}","name":"S","file":"s.wav"}}"#),
            })
            .collect();
        format!(r#"{{"sounds":[{}]}}"#, items.join(","))
    }

    fn colors(c: &Config) -> Vec<&str> {
        c.sounds.iter().map(|s| s.color.as_deref().unwrap()).collect()
    }

    #[test]
    fn sounds_without_color_get_distinct_palette_colors_at_load() {
        let c = load_json(&sounds_json(&[None; 10]));
        let got = colors(&c);
        assert_eq!(got, PALETTE.to_vec(), "palette order, each once");
        let c = load_json(&sounds_json(&[None; 11]));
        let got = colors(&c);
        assert_eq!(&got[..10], &PALETTE[..]);
        assert_eq!(got[10], PALETTE[0], "11th repeats the least used (earliest)");
    }

    #[test]
    fn invalid_color_is_replaced_and_valid_lowercase_is_uppercased() {
        let c = load_json(&sounds_json(&[Some("red"), Some("#12345"), Some("#GGGGGG"), Some("#abcdef"), Some("#F4A261")]));
        let got = colors(&c);
        assert_eq!(got[3], "#ABCDEF", "valid hex is kept, uppercased, even off-palette");
        assert_eq!(got[4], "#F4A261");
        for bad in &got[..3] {
            assert!(PALETTE.contains(bad), "{bad} replaced by a palette color");
        }
        assert_eq!(&got[..3], &["#FF595E", "#FF924C", "#FFCA3A"]);
    }

    #[test]
    fn assigned_colors_skip_ones_already_in_use() {
        let c = load_json(&sounds_json(&[Some("#FF595E"), None, Some("#FF924C"), None]));
        assert_eq!(colors(&c), ["#FF595E", "#FFCA3A", "#FF924C", "#8AC926"]);
    }

    #[test]
    fn sound_lookup_by_id() {
        let mut cfg = Config::default();
        cfg.sounds.push(Sound { id: "a".into(), name: "A".into(), file: "a.wav".into(), image: None, volume: 1.0, hotkey: None, color: None, trim: None });
        assert_eq!(cfg.sound("a").unwrap().name, "A");
        assert!(cfg.sound("zzz").is_none());
        cfg.sound_mut("a").unwrap().name = "B".into();
        assert_eq!(cfg.sound("a").unwrap().name, "B");
    }

    #[test]
    fn invalid_trims_are_dropped_at_load() {
        let json = r#"{"sounds":[
            {"id":"ok","name":"S","file":"s.wav","trim":{"start_secs":0.5,"end_secs":2.0}},
            {"id":"neg","name":"S","file":"s.wav","trim":{"start_secs":-0.1,"end_secs":2.0}},
            {"id":"short","name":"S","file":"s.wav","trim":{"start_secs":1.0,"end_secs":1.04}},
            {"id":"backwards","name":"S","file":"s.wav","trim":{"start_secs":2.0,"end_secs":1.0}},
            {"id":"huge","name":"S","file":"s.wav","trim":{"start_secs":0.0,"end_secs":1e40}},
            {"id":"null","name":"S","file":"s.wav","trim":null}]}"#;
        let c = load_json(json);
        let trims: Vec<Option<Trim>> = c.sounds.iter().map(|s| s.trim).collect();
        assert_eq!(trims[0], Some(Trim { start_secs: 0.5, end_secs: 2.0 }));
        assert!(trims[1..].iter().all(Option::is_none), "{trims:?}");
    }

    #[test]
    fn trim_validity_needs_finite_ordered_and_min_length() {
        assert!(Trim { start_secs: 0.0, end_secs: 0.05 }.is_valid());
        assert!(Trim { start_secs: 1.0, end_secs: 1.05 }.is_valid(), "f32 rounding of a 50 ms range");
        assert!(!Trim { start_secs: 0.0, end_secs: 0.049 }.is_valid());
        assert!(!Trim { start_secs: f32::NAN, end_secs: 1.0 }.is_valid());
        assert!(!Trim { start_secs: 0.0, end_secs: f32::INFINITY }.is_valid());
    }
}
