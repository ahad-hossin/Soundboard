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
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use state::{emit_state, emit_toast, lock, App, Ctx, Shared, CACHE_BYTES};

pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![STARTUP_ARG]),
        ))
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let to_tray = window
                    .app_handle()
                    .try_state::<Ctx>()
                    .is_some_and(|ctx| lock(&ctx.shared).config.startup.close_to_tray);
                if to_tray {
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        log::warn!("hiding the window to the tray failed: {e}");
                    }
                }
            }
        })
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
                microphone: cfg.devices.microphone.clone(),
                mic_gain: cfg.devices.mic_gain,
            };
            let mut engine = Engine::new(RodioBackend::new(), outputs, cfg.master_volume, CACHE_BYTES);
            // `Engine::new` already read the default output; this starts mic passthrough once.
            let _ = engine.apply_passthrough();
            let available_devices = engine.devices();
            let available_inputs = engine.input_devices();
            let mut app_state = App::new(cfg, engine, library, available_devices, available_inputs);
            notices.extend(app_state.passthrough_toast());
            app_state.startup_notices = notices;
            let shared: Shared = Arc::new(Mutex::new(app_state));
            app.manage(Ctx { shared: shared.clone(), hub: Hub::new(), remote: Mutex::new(None) });
            state::spawn_warm(shared, None);

            let handle = app.handle().clone();
            sync_autostart(&handle);
            let tray_ok = match build_tray(&handle) {
                Ok(()) => true,
                Err(e) => {
                    log::error!("tray icon failed: {e}");
                    false
                }
            };
            // The window is created hidden (tauri.conf.json), so a minimized start never flashes.
            // Without a tray icon there would be no way back to a hidden window, so it shows anyway.
            if !(tray_ok && start_hidden(&handle)) {
                show_main_window(&handle);
            }
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
            commands::set_sound_color,
            commands::delete_sound,
            commands::set_sound_image,
            commands::clear_sound_image,
            commands::reorder_sounds,
            commands::set_hotkey,
            commands::clear_hotkey,
            commands::set_stop_all_hotkey,
            commands::set_devices,
            commands::set_mic_gain,
            commands::refresh_devices,
            commands::test_tone,
            commands::set_remote_port,
            commands::set_pin,
            commands::restart_remote,
            commands::set_startup,
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

/// Argument the autostart entry launches the app with.
const STARTUP_ARG: &str = "--minimized";

/// True when the main window should stay hidden at launch: the option is on, or the app was
/// started by the Windows startup entry.
fn start_hidden(app: &AppHandle) -> bool {
    let option = lock(&app.state::<Ctx>().shared).config.startup.start_minimized;
    option || std::env::args().any(|a| a == STARTUP_ARG)
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Makes the Windows Run key entry match the saved setting. A failure is logged, never fatal.
fn sync_autostart(app: &AppHandle) {
    let want = lock(&app.state::<Ctx>().shared).config.startup.autostart;
    let launcher = app.autolaunch();
    if launcher.is_enabled().unwrap_or(false) != want {
        let result = if want { launcher.enable() } else { launcher.disable() };
        if let Err(e) = result {
            log::warn!("could not sync the Windows startup entry: {e}");
        }
    }
}

/// Tray icon with the app icon, a Show / Stop All / Quit menu, and left click to show the window.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "stop_all", "Stop All", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &stop_all, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Soundboard")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "stop_all" => {
                let ctx = app.state::<Ctx>();
                lock(&ctx.shared).engine.stop_all();
                emit_state(app, &ctx);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Every 100 ms drop finished sinks, write pending config changes, restart a mic passthrough
/// that stopped on its own, and push state when the playing set changed. A save failure is
/// toasted once until a save succeeds; a failed passthrough restart once per distinct error.
fn spawn_reaper(app: AppHandle) {
    std::thread::Builder::new()
        .name("reaper".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let (changed, save_error, passthrough_error) = {
                let ctx = app.state::<Ctx>();
                let mut a = lock(&ctx.shared);
                let changed = a.engine.reap();
                let save_error = match a.flush() {
                    Ok(()) => None,
                    Err(e) if !a.save_failed_reported => {
                        log::error!("config save failed: {e}");
                        a.save_failed_reported = true;
                        Some(e)
                    }
                    Err(_) => None,
                };
                // The restart is a round trip to the audio thread; it only happens after a loss.
                let passthrough_error = if a.engine.passthrough_lost() {
                    log::warn!("microphone passthrough stopped, restarting");
                    let _ = a.engine.ensure_passthrough();
                    a.passthrough_toast_with("Microphone passthrough stopped, restarting failed")
                } else {
                    None
                };
                (changed, save_error, passthrough_error)
            };
            if let Some(e) = save_error {
                emit_toast(&app, "error", format!("Could not save settings: {e}"));
            }
            if let Some(t) = passthrough_error {
                emit_toast(&app, "error", t);
            }
            if changed {
                let ctx = app.state::<Ctx>();
                emit_state(&app, &ctx);
            }
        })
        .expect("spawn reaper");
}
