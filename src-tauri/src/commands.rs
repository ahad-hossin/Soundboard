use std::collections::HashSet;
use std::path::Path;

use soundboard_core::audio::DeviceTarget;
use soundboard_core::config::{self, Devices};
use soundboard_core::hotkeys::{self, Owner};
use soundboard_core::library::Library;
use soundboard_core::palette::is_palette_color;
use soundboard_core::remote::auth;
use tauri_plugin_autostart::ManagerExt;
use tauri::{AppHandle, State};

use crate::state::{emit_state, emit_toast, lan_addresses, lock, spawn_warm, App, Ctx, UiState};
use crate::{hotkeys_glue, remote_glue};

/// Lock, mutate, unlock. The caller emits state afterwards.
fn mutate<T>(ctx: &Ctx, f: impl FnOnce(&mut App) -> Result<T, String>) -> Result<T, String> {
    let mut app = lock(&ctx.shared);
    f(&mut app)
}

/// Writes security- and data-relevant changes right away instead of on the next reaper tick.
/// A failure leaves the config dirty, so the reaper retries and shows the error toast.
fn flush_now(a: &mut App) {
    if let Err(e) = a.flush() {
        log::error!("config save failed: {e}");
    }
}

fn rebind_hotkeys(app: &AppHandle) {
    for e in hotkeys_glue::register_all(app) {
        emit_toast(app, "warn", e);
    }
}

#[tauri::command]
pub fn get_state(ctx: State<'_, Ctx>) -> UiState {
    let state = lock(&ctx.shared).ui_state(&ctx.hub);
    lock(&ctx.shared).startup_notices.clear();
    state
}

/// Async so the first play (decode, device open) never blocks the main thread; the lock
/// is taken only inside the blocking task.
#[tauri::command]
pub async fn play(id: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let shared = ctx.shared.clone();
    let result = tauri::async_runtime::spawn_blocking(move || lock(&shared).play(&id))
        .await
        .map_err(|e| format!("Playing the sound failed: {e}"));
    // Restart-on-replay may have stopped the previous sinks before failing, so push state either way.
    let toasts = match result.and_then(|r| r) {
        Ok(t) => t,
        Err(e) => {
            emit_state(&app, &ctx);
            return Err(e);
        }
    };
    for t in toasts {
        emit_toast(&app, &t.level, t.message);
    }
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn stop_all(app: AppHandle, ctx: State<'_, Ctx>) {
    lock(&ctx.shared).engine.stop_all();
    emit_state(&app, &ctx);
}

#[tauri::command]
pub fn set_master_volume(value: f32, app: AppHandle, ctx: State<'_, Ctx>) {
    {
        let mut a = lock(&ctx.shared);
        a.engine.set_master(value);
        a.config.master_volume = value.clamp(0.0, 1.0);
        a.mark_dirty();
    }
    emit_state(&app, &ctx);
}

#[tauri::command]
pub fn set_sound_volume(id: String, value: f32, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        if a.set_sound_volume(&id, value) {
            Ok(())
        } else {
            Err("That sound no longer exists".to_string())
        }
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

/// Imports each path; returns one message per rejected file.
#[tauri::command]
pub async fn add_sounds(paths: Vec<String>, app: AppHandle, ctx: State<'_, Ctx>) -> Result<Vec<String>, String> {
    // Short lock just to learn where the library lives; the heavy import runs unlocked.
    // The current colors come along so new sounds get the least-used ones.
    let (root, mut used_colors) = {
        let a = lock(&ctx.shared);
        let colors: Vec<String> = a.config.sounds.iter().filter_map(|s| s.color.clone()).collect();
        (a.library.root().to_path_buf(), colors)
    };
    let (imported, rejected) = tauri::async_runtime::spawn_blocking(move || {
        let mut imported = Vec::new();
        let mut rejected = Vec::new();
        match Library::open(root) {
            Ok(library) => {
                for p in &paths {
                    match library.import_sound(Path::new(p), &used_colors.iter().map(String::as_str).collect::<Vec<_>>()) {
                        Ok(s) => {
                            used_colors.extend(s.color.clone());
                            imported.push(s);
                        }
                        Err(e) => rejected.push(e.to_string()),
                    }
                }
            }
            Err(e) => rejected.push(format!("Could not open the sound library: {e}")),
        }
        (imported, rejected)
    })
    .await
    .map_err(|e| format!("Importing sounds failed: {e}"))?;
    if !imported.is_empty() {
        let ids: Vec<String> = imported.iter().map(|s| s.id.clone()).collect();
        {
            let mut a = lock(&ctx.shared);
            a.config.sounds.extend(imported);
            a.mark_dirty();
            a.refresh_files();
        }
        spawn_warm(ctx.shared.clone(), Some(ids));
    }
    emit_state(&app, &ctx);
    Ok(rejected)
}

#[tauri::command]
pub fn rename_sound(id: String, name: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    mutate(&ctx, |a| {
        a.config.sound_mut(&id).ok_or("That sound no longer exists")?.name = name;
        a.mark_dirty();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn set_sound_color(id: String, color: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    if !is_palette_color(&color) {
        return Err("That color is not in the palette".into());
    }
    mutate(&ctx, |a| {
        a.config.sound_mut(&id).ok_or("That sound no longer exists")?.color = Some(color.to_ascii_uppercase());
        a.mark_dirty();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn delete_sound(id: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let idx = a.config.sounds.iter().position(|s| s.id == id).ok_or("That sound no longer exists")?;
        let sound = a.config.sounds.remove(idx);
        if let Err(e) = a.library.delete_sound(&sound) {
            log::warn!("delete files failed: {e}");
        }
        a.engine.invalidate(&id);
        a.mark_dirty();
        flush_now(a);
        a.refresh_files();
        Ok(())
    })?;
    rebind_hotkeys(&app);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn set_sound_image(id: String, path: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let mut sound = a.config.sound(&id).cloned().ok_or("That sound no longer exists")?;
        a.library.import_image(&mut sound, Path::new(&path)).map_err(|e| e.to_string())?;
        *a.config.sound_mut(&id).ok_or("That sound no longer exists")? = sound;
        a.mark_dirty();
        a.refresh_files();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn clear_sound_image(id: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let mut sound = a.config.sound(&id).cloned().ok_or("That sound no longer exists")?;
        a.library.remove_image(&mut sound).map_err(|e| e.to_string())?;
        *a.config.sound_mut(&id).ok_or("That sound no longer exists")? = sound;
        a.mark_dirty();
        a.refresh_files();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn reorder_sounds(ids: Vec<String>, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let unique: HashSet<&String> = ids.iter().collect();
        if unique.len() != ids.len()
            || ids.len() != a.config.sounds.len()
            || !ids.iter().all(|id| a.config.sound(id).is_some())
        {
            return Err("The sound list changed, try again".into());
        }
        let mut old = std::mem::take(&mut a.config.sounds);
        let mut reordered = Vec::with_capacity(old.len());
        for id in &ids {
            if let Some(i) = old.iter().position(|s| &s.id == id) {
                reordered.push(old.remove(i));
            }
        }
        if !old.is_empty() {
            // Defense in depth: never drop sounds. Restore the original order and fail.
            old.extend(reordered);
            a.config.sounds = old;
            return Err("The sound list changed, try again".into());
        }
        a.config.sounds = reordered;
        a.mark_dirty();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn set_hotkey(id: String, accelerator: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let accel = hotkeys::validate_binding(&a.config, &accelerator, Owner::Sound(&id)).map_err(|e| e.to_string())?;
        a.config.sound_mut(&id).ok_or("That sound no longer exists")?.hotkey = Some(accel);
        a.mark_dirty();
        Ok(())
    })?;
    rebind_hotkeys(&app);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn clear_hotkey(id: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        a.config.sound_mut(&id).ok_or("That sound no longer exists")?.hotkey = None;
        a.mark_dirty();
        Ok(())
    })?;
    rebind_hotkeys(&app);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn set_stop_all_hotkey(accelerator: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let accel = hotkeys::validate_binding(&a.config, &accelerator, Owner::StopAll).map_err(|e| e.to_string())?;
        a.config.stop_all_hotkey = accel;
        a.mark_dirty();
        Ok(())
    })?;
    rebind_hotkeys(&app);
    emit_state(&app, &ctx);
    Ok(())
}

/// Switches all three device slots and restarts mic passthrough (inside `set_outputs`).
#[tauri::command]
pub fn set_devices(
    speakers: Option<String>,
    virtual_mic: Option<String>,
    microphone: Option<String>,
    app: AppHandle,
    ctx: State<'_, Ctx>,
) {
    let toast = {
        let mut a = lock(&ctx.shared);
        a.config.devices = Devices {
            speakers: speakers.filter(|s| !s.is_empty()),
            virtual_mic: virtual_mic.filter(|s| !s.is_empty()),
            microphone: microphone.filter(|s| !s.is_empty()),
            mic_gain: a.config.devices.mic_gain,
        };
        let outputs = a.outputs();
        a.engine.set_outputs(outputs);
        // `set_outputs` already re-read the default device; only the cached lists need updating.
        a.refresh_device_lists();
        a.warned_devices.clear();
        a.mark_dirty();
        a.passthrough_toast()
    };
    if let Some(t) = toast {
        emit_toast(&app, "error", t);
    }
    emit_state(&app, &ctx);
}

#[tauri::command]
pub fn set_mic_gain(value: f32, app: AppHandle, ctx: State<'_, Ctx>) {
    {
        let mut a = lock(&ctx.shared);
        a.engine.set_mic_gain(value);
        a.config.devices.mic_gain = a.engine.outputs().mic_gain;
        a.mark_dirty();
    }
    emit_state(&app, &ctx);
}

/// Re-enumerates devices (slow) and restarts mic passthrough; the Settings panel calls this when opened.
#[tauri::command]
pub fn refresh_devices(app: AppHandle, ctx: State<'_, Ctx>) {
    let addresses = lan_addresses();
    let toast = {
        let mut a = lock(&ctx.shared);
        a.available_devices = a.engine.refresh_devices();
        a.available_inputs = a.engine.input_devices();
        a.addresses = addresses;
        a.refresh_files();
        a.passthrough_toast()
    };
    if let Some(t) = toast {
        emit_toast(&app, "error", t);
    }
    emit_state(&app, &ctx);
}

#[tauri::command]
pub fn test_tone(device: Option<String>, ctx: State<'_, Ctx>) -> Result<(), String> {
    lock(&ctx.shared)
        .engine
        .test_tone(&DeviceTarget::from_name(device.as_deref()))
        .map(|_| ())
        .map_err(|e| format!("Test tone failed: {e}"))
}

#[tauri::command]
pub fn set_remote_port(port: u16, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    if port < 1024 {
        return Err("Port must be between 1024 and 65535".into());
    }
    {
        let mut a = lock(&ctx.shared);
        a.config.remote.port = port;
        a.mark_dirty();
        flush_now(&mut a);
    }
    remote_glue::start(&app);
    emit_state(&app, &ctx);
    Ok(())
}

/// `Some(pin)` sets a PIN, `None` removes it. Either way every phone token is invalidated.
#[tauri::command]
pub fn set_pin(pin: Option<String>, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let hash = match pin.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) if !auth::valid_pin_format(p) => return Err("PIN must be 4 to 8 digits".into()),
        Some(p) => Some(auth::hash_pin(p)),
        None => None,
    };
    {
        let mut a = lock(&ctx.shared);
        a.config.remote.pin_hash = hash;
        a.config.remote.token_secret = config::new_secret();
        a.mark_dirty();
        flush_now(&mut a);
    }
    ctx.hub.kick_all();
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn restart_remote(app: AppHandle) {
    remote_glue::start(&app);
}

/// Saves the three Startup options and makes the Windows Run key entry match `autostart`.
/// A registry failure is reported, but the other two options are still saved.
#[tauri::command]
pub fn set_startup(
    autostart: bool,
    start_minimized: bool,
    close_to_tray: bool,
    app: AppHandle,
    ctx: State<'_, Ctx>,
) -> Result<(), String> {
    // A dev build would register target\debug\soundboard.exe in the Run key and keep starting
    // that binary at login even after the installer is used, so only the installed app may enable it.
    if autostart && cfg!(debug_assertions) {
        return Err("Start with Windows is only available in the installed app, not in dev mode".into());
    }
    let launcher = app.autolaunch();
    let result = if autostart { launcher.enable() } else { launcher.disable() };
    // Keep the stored flag equal to the real registration so the next launch does not retry.
    let registered = match &result {
        Ok(()) => autostart,
        Err(_) => launcher.is_enabled().unwrap_or(!autostart),
    };
    {
        let mut a = lock(&ctx.shared);
        a.config.startup.start_minimized = start_minimized;
        a.config.startup.close_to_tray = close_to_tray;
        a.config.startup.autostart = registered;
        a.mark_dirty();
        flush_now(&mut a);
    }
    emit_state(&app, &ctx);
    result.map_err(|e| format!("Could not change the Windows startup entry: {e}"))
}
