mod commands;
mod hotkeys_glue;
mod remote_glue;
mod state;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use soundboard_core::audio::{Engine, RodioBackend};
use soundboard_core::config::{self, LoadOutcome};
use soundboard_core::library::Library;
use soundboard_core::remote::Hub;
use tauri::{AppHandle, Manager, RunEvent};

use state::{emit_state, emit_toast, lock, App, Ctx, Shared, CACHE_BYTES};

pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let library = Library::open(&data_dir)?;
            let outcome = config::load(&library.config_path());
            let mut notices = Vec::new();
            let cfg = match outcome {
                LoadOutcome::Loaded(c) => c,
                LoadOutcome::Fresh(c) => {
                    config::save(&library.config_path(), &c)?;
                    c
                }
                LoadOutcome::Recovered { config, backup } => {
                    notices.push(format!("Settings file was unreadable and has been reset. A copy is at {}", backup.display()));
                    config
                }
            };
            let outputs = soundboard_core::audio::Outputs {
                speakers: cfg.devices.speakers.clone(),
                virtual_mic: cfg.devices.virtual_mic.clone(),
            };
            let mut engine = Engine::new(RodioBackend::new(), outputs, cfg.master_volume, CACHE_BYTES);
            let available_devices = engine.refresh_devices();
            let mut app_state = App::new(cfg, engine, library, available_devices);
            app_state.startup_notices = notices;
            let shared: Shared = Arc::new(Mutex::new(app_state));
            app.manage(Ctx { shared, hub: Hub::new(), remote: Mutex::new(None) });

            let handle = app.handle().clone();
            {
                let h = handle.clone();
                app.state::<Ctx>().hub.set_on_clients_changed(Box::new(move || {
                    let ctx = h.state::<Ctx>();
                    emit_state(&h, &ctx);
                }));
            }
            let errors = hotkeys_glue::register_all(&handle);
            lock(&handle.state::<Ctx>().shared).startup_notices.extend(errors);
            remote_glue::start(&handle);
            spawn_reaper(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::play,
            commands::stop_all,
            commands::set_master_volume,
            commands::set_sound_volume,
            commands::add_sounds,
            commands::rename_sound,
            commands::delete_sound,
            commands::set_sound_image,
            commands::clear_sound_image,
            commands::reorder_sounds,
            commands::set_hotkey,
            commands::clear_hotkey,
            commands::set_stop_all_hotkey,
            commands::set_devices,
            commands::refresh_devices,
            commands::test_tone,
            commands::set_remote_port,
            commands::set_pin,
            commands::restart_remote,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                if let Some(ctx) = app.try_state::<Ctx>() {
                    if let Err(e) = lock(&ctx.shared).flush() {
                        log::error!("config save on exit failed: {e}");
                    }
                }
            }
        });
}

/// Every 100 ms drop finished sinks, write pending config changes, and push state
/// when the playing set changed. A save failure is toasted once until a save succeeds.
fn spawn_reaper(app: AppHandle) {
    std::thread::Builder::new()
        .name("reaper".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let (changed, save_error) = {
                let ctx = app.state::<Ctx>();
                let mut a = lock(&ctx.shared);
                let changed = a.engine.reap();
                let was_dirty = a.dirty;
                let save_error = match a.flush() {
                    Ok(()) => {
                        if was_dirty {
                            a.save_failed_reported = false;
                        }
                        None
                    }
                    Err(e) if !a.save_failed_reported => {
                        log::error!("config save failed: {e}");
                        a.save_failed_reported = true;
                        Some(e)
                    }
                    Err(_) => None,
                };
                (changed, save_error)
            };
            if let Some(e) = save_error {
                emit_toast(&app, "error", format!("Could not save settings: {e}"));
            }
            if changed {
                let ctx = app.state::<Ctx>();
                emit_state(&app, &ctx);
            }
        })
        .expect("spawn reaper");
}
