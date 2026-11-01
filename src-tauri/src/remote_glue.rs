use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use soundboard_core::remote::auth::Lockout;
use soundboard_core::remote::protocol::ServerMsg;
use soundboard_core::remote::{self, RemoteHandler};
use tauri::{AppHandle, Manager};

use crate::state::{emit_state, emit_toast, lan_addresses, lock, play_and_report, Ctx};

struct Handler {
    app: AppHandle,
}

impl RemoteHandler for Handler {
    fn snapshot(&self) -> ServerMsg {
        lock(&self.app.state::<Ctx>().shared).remote_snapshot()
    }

    fn play(&self, id: &str) {
        // Called from the server's async task; playing can block, so hand it off.
        let (app, id) = (self.app.clone(), id.to_string());
        tauri::async_runtime::spawn_blocking(move || play_and_report(&app, &id));
    }

    fn stop_all(&self) {
        let ctx = self.app.state::<Ctx>();
        lock(&ctx.shared).engine.stop_all();
        emit_state(&self.app, &ctx);
    }

    fn set_master_volume(&self, value: f32) {
        let ctx = self.app.state::<Ctx>();
        {
            let mut a = lock(&ctx.shared);
            a.engine.set_master(value);
            a.config.master_volume = value.clamp(0.0, 1.0);
            a.mark_dirty();
        }
        emit_state(&self.app, &ctx);
    }

    fn set_sound_volume(&self, id: &str, value: f32) {
        let ctx = self.app.state::<Ctx>();
        let known = lock(&ctx.shared).set_sound_volume(id, value);
        if known {
            emit_state(&self.app, &ctx);
        } else {
            log::debug!("remote set_sound_volume for unknown sound {id}");
        }
    }

    fn image_path(&self, id: &str) -> Option<PathBuf> {
        let ctx = self.app.state::<Ctx>();
        let a = lock(&ctx.shared);
        a.config.sound(id).and_then(|s| a.library.image_path(s))
    }

    fn pin_hash(&self) -> Option<String> {
        lock(&self.app.state::<Ctx>().shared).config.remote.pin_hash.clone()
    }

    fn token_secret(&self) -> String {
        lock(&self.app.state::<Ctx>().shared).config.remote.token_secret.clone()
    }
}

/// (Re)starts the LAN server on the configured port. Safe to call any time.
pub fn start(app: &AppHandle) {
    let ctx = app.state::<Ctx>();
    let old = ctx.remote.lock().unwrap().take();
    let addresses = lan_addresses();
    let port = {
        let mut a = lock(&ctx.shared);
        a.remote_port = None;
        a.addresses = addresses;
        a.config.remote.port
    };
    let handler: Arc<dyn RemoteHandler> = Arc::new(Handler { app: app.clone() });
    let router = remote::router(handler, ctx.hub.clone(), Arc::new(Mutex::new(Lockout::standard())));

    let app2 = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let ctx = app2.state::<Ctx>();
        if let Some(h) = old {
            // Wait for the old listener to be dropped, or bind could silently land on port+1.
            h.abort();
            let _ = h.await;
            // The old server has no graceful shutdown; close its phones so they reconnect.
            ctx.hub.kick_all();
        }
        match remote::bind(port).await {
            Ok((listener, actual)) => {
                {
                    let mut a = lock(&ctx.shared);
                    a.remote_port = Some(actual);
                    a.remote_error = None;
                }
                emit_state(&app2, &ctx);
                if let Err(e) = remote::serve(listener, router).await {
                    let mut a = lock(&ctx.shared);
                    a.remote_port = None;
                    a.remote_error = Some(format!("Remote server stopped: {e}"));
                    drop(a);
                    emit_state(&app2, &ctx);
                }
            }
            Err(e) => {
                {
                    let mut a = lock(&ctx.shared);
                    a.remote_port = None;
                    a.remote_error = Some(format!("Could not open port {port} or the next ten: {e}"));
                }
                emit_toast(&app2, "error", "The phone remote could not start. Check the Remote panel.");
                emit_state(&app2, &ctx);
            }
        }
    });
    *ctx.remote.lock().unwrap() = Some(handle);
}
