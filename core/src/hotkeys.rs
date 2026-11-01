//! Accelerator strings: `Ctrl+Alt+Shift+Super+<KeyboardEvent.code>` in that fixed order.
//! This is the format `tauri-plugin-global-shortcut` parses.

use thiserror::Error;

use crate::config::Config;

pub const STOP_ALL_OWNER: &str = "Stop All";
const MODIFIER_ORDER: [&str; 4] = ["Ctrl", "Alt", "Shift", "Super"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner<'a> {
    Sound(&'a str),
    StopAll,
}

#[derive(Debug, Error, PartialEq)]
pub enum HotkeyError {
    #[error("{0}")]
    Invalid(String),
    #[error("{accelerator} is already used by {owner}")]
    Conflict { accelerator: String, owner: String },
}

fn is_valid_code(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}

pub fn normalize(input: &str) -> Result<String, HotkeyError> {
    let invalid = || HotkeyError::Invalid(format!("\"{input}\" is not a valid hotkey"));
    let parts: Vec<&str> = input.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err(invalid());
    }
    let mut mods = [false; 4];
    let mut key: Option<&str> = None;
    for p in &parts {
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmdorctrl" | "commandorcontrol" => mods[0] = true,
            "alt" | "option" => mods[1] = true,
            "shift" => mods[2] = true,
            "super" | "meta" | "win" | "cmd" | "command" => mods[3] = true,
            _ => {
                if key.is_some() {
                    return Err(invalid());
                }
                key = Some(p);
            }
        }
    }
    let key = key.ok_or_else(invalid)?;
    if !is_valid_code(key) {
        return Err(invalid());
    }
    let mut out: Vec<&str> = MODIFIER_ORDER.iter().zip(mods).filter(|(_, on)| *on).map(|(m, _)| *m).collect();
    out.push(key);
    Ok(out.join("+"))
}

/// Only F1..F24 may be bound without a modifier.
pub fn requires_modifier(key_code: &str) -> bool {
    let is_fkey = key_code
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .map(|n| (1..=24).contains(&n))
        .unwrap_or(false);
    !is_fkey
}

pub fn find_conflict(cfg: &Config, accelerator: &str, exclude: Owner) -> Option<String> {
    if exclude != Owner::StopAll && cfg.stop_all_hotkey == accelerator {
        return Some(STOP_ALL_OWNER.to_string());
    }
    cfg.sounds
        .iter()
        .filter(|s| exclude != Owner::Sound(&s.id))
        .find(|s| s.hotkey.as_deref() == Some(accelerator))
        .map(|s| s.name.clone())
}

/// Normalizes, enforces the modifier rule, and checks for conflicts. Returns the canonical accelerator.
pub fn validate_binding(cfg: &Config, input: &str, exclude: Owner) -> Result<String, HotkeyError> {
    let accel = normalize(input)?;
    let key = accel.rsplit('+').next().unwrap_or("");
    if !accel.contains('+') && requires_modifier(key) {
        return Err(HotkeyError::Invalid(format!("{accel} needs a modifier such as Ctrl or Alt")));
    }
    if let Some(owner) = find_conflict(cfg, &accel, exclude) {
        return Err(HotkeyError::Conflict { accelerator: accel, owner });
    }
    Ok(accel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Sound};

    fn cfg() -> Config {
        let mut c = Config::default();
        c.stop_all_hotkey = "Ctrl+Shift+F12".into();
        c.sounds.push(Sound { id: "a".into(), name: "Airhorn".into(), file: "a.wav".into(), image: None, volume: 1.0, hotkey: Some("Ctrl+Shift+Digit1".into()), color: None, trim: None });
        c.sounds.push(Sound { id: "b".into(), name: "Bruh".into(), file: "b.wav".into(), image: None, volume: 1.0, hotkey: None, color: None, trim: None });
        c
    }

    #[test]
    fn normalize_orders_modifiers_and_title_cases() {
        assert_eq!(normalize("shift+ctrl+Digit1").unwrap(), "Ctrl+Shift+Digit1");
        assert_eq!(normalize(" alt + KeyQ ").unwrap(), "Alt+KeyQ");
        assert_eq!(normalize("Super+Alt+Ctrl+Shift+KeyZ").unwrap(), "Ctrl+Alt+Shift+Super+KeyZ");
    }

    #[test]
    fn normalize_accepts_aliases() {
        assert_eq!(normalize("CmdOrCtrl+KeyA").unwrap(), "Ctrl+KeyA");
        assert_eq!(normalize("Control+KeyA").unwrap(), "Ctrl+KeyA");
        assert_eq!(normalize("Meta+KeyA").unwrap(), "Super+KeyA");
        assert_eq!(normalize("Win+KeyA").unwrap(), "Super+KeyA");
        assert_eq!(normalize("Option+KeyA").unwrap(), "Alt+KeyA");
    }

    #[test]
    fn normalize_rejects_bad_input() {
        assert!(matches!(normalize(""), Err(HotkeyError::Invalid(_))));
        assert!(matches!(normalize("Ctrl+Shift"), Err(HotkeyError::Invalid(_))), "no key");
        assert!(matches!(normalize("Ctrl+KeyA+KeyB"), Err(HotkeyError::Invalid(_))), "two keys");
        assert!(matches!(normalize("Ctrl+a"), Err(HotkeyError::Invalid(_))), "not a KeyboardEvent.code");
        assert!(matches!(normalize("Ctrl+Key A"), Err(HotkeyError::Invalid(_))));
    }

    #[test]
    fn f_keys_do_not_require_modifier() {
        assert!(!requires_modifier("F9"));
        assert!(!requires_modifier("F24"));
        assert!(requires_modifier("F25"));
        assert!(requires_modifier("KeyA"));
        assert!(requires_modifier("Digit1"));
        assert_eq!(validate_binding(&cfg(), "F9", Owner::Sound("b")).unwrap(), "F9");
        assert!(matches!(validate_binding(&cfg(), "KeyA", Owner::Sound("b")), Err(HotkeyError::Invalid(_))));
    }

    #[test]
    fn conflict_with_sound_names_owner() {
        assert_eq!(find_conflict(&cfg(), "Ctrl+Shift+Digit1", Owner::Sound("b")), Some("Airhorn".into()));
        let err = validate_binding(&cfg(), "shift+ctrl+Digit1", Owner::Sound("b")).unwrap_err();
        assert_eq!(err.to_string(), "Ctrl+Shift+Digit1 is already used by Airhorn");
    }

    #[test]
    fn conflict_with_stop_all() {
        assert_eq!(find_conflict(&cfg(), "Ctrl+Shift+F12", Owner::Sound("b")), Some(STOP_ALL_OWNER.into()));
    }

    #[test]
    fn rebinding_self_is_not_a_conflict() {
        assert_eq!(find_conflict(&cfg(), "Ctrl+Shift+Digit1", Owner::Sound("a")), None);
        assert_eq!(find_conflict(&cfg(), "Ctrl+Shift+F12", Owner::StopAll), None);
        assert_eq!(validate_binding(&cfg(), "Ctrl+Shift+F12", Owner::StopAll).unwrap(), "Ctrl+Shift+F12");
    }

    #[test]
    fn free_key_is_fine() {
        assert_eq!(find_conflict(&cfg(), "Ctrl+Shift+Digit2", Owner::Sound("b")), None);
    }
}
