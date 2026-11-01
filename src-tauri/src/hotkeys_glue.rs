use std::str::FromStr;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::{emit_state, lock, play_and_report, Ctx};

#[derive(Clone)]
enum Action {
    Play(String),
    StopAll,
}

/// Unregisters everything and registers Stop All plus every sound hotkey from config.
/// Returns one user-facing message per accelerator the OS or parser refused.
pub fn register_all(app: &AppHandle) -> Vec<String> {
    let ctx = app.state::<Ctx>();
    let gs = app.global_shortcut();
    if let Err(e) = gs.unregister_all() {
        log::warn!("unregister_all failed: {e}");
    }

    let bindings: Vec<(String, Action)> = {
        let a = lock(&ctx.shared);
        let mut v = vec![(a.config.stop_all_hotkey.clone(), Action::StopAll)];
        v.extend(a.config.sounds.iter().filter_map(|s| s.hotkey.clone().map(|h| (h, Action::Play(s.id.clone())))));
        v
    };

    let mut errors = Vec::new();
    for (accel, action) in bindings {
        let shortcut = match Shortcut::from_str(&accel) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("Hotkey {accel} could not be parsed: {e}"));
                continue;
            }
        };
        let result = gs.on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            match &action {
                Action::StopAll => {
                    let ctx = app.state::<Ctx>();
                    lock(&ctx.shared).engine.stop_all();
                    emit_state(app, &ctx);
                }
                Action::Play(id) => {
                    // This callback runs on the main thread; playing can block, so hand it off.
                    let (app, id) = (app.clone(), id.clone());
                    tauri::async_runtime::spawn_blocking(move || play_and_report(&app, &id));
                }
            }
        });
        if let Err(e) = result {
            errors.push(format!("Hotkey {accel} is taken by another program ({e})"));
        }
    }
    errors
}
