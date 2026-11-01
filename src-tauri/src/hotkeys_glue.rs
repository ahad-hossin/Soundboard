use std::str::FromStr;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::{emit_state, emit_toast, lock, Ctx};

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
            let ctx = app.state::<Ctx>();
            match &action {
                Action::StopAll => lock(&ctx.shared).engine.stop_all(),
                Action::Play(id) => {
                    let result = lock(&ctx.shared).play(id);
                    match result {
                        Ok(toasts) => {
                            for t in toasts {
                                emit_toast(app, &t.level, t.message);
                            }
                        }
                        Err(e) => emit_toast(app, "error", e),
                    }
                }
            }
            emit_state(app, &ctx);
        });
        if let Err(e) = result {
            errors.push(format!("Hotkey {accel} is taken by another program ({e})"));
        }
    }
    errors
}
