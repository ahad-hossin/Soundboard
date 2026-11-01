//! LAN remote: axum server that serves the phone page and a WebSocket mirror of app state.

pub mod auth;
pub mod protocol;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path as AxumPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use auth::Lockout;
use protocol::{ClientMsg, ServerMsg};

pub const PAGE_HTML: &str = include_str!("page.html");
pub const MANIFEST_JSON: &str = include_str!("manifest.json");
pub const ICON_SVG: &str = include_str!("icon.svg");
/// The requested port plus the next ten.
pub const PORT_TRIES: u16 = 11;

/// What the server needs from the application. Implemented by the Tauri app crate.
pub trait RemoteHandler: Send + Sync + 'static {
    fn snapshot(&self) -> ServerMsg;
    fn play(&self, id: &str);
    fn stop_all(&self);
    fn set_master_volume(&self, value: f32);
    fn image_path(&self, id: &str) -> Option<PathBuf>;
    fn pin_hash(&self) -> Option<String>;
    fn token_secret(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum HubEvent {
    State(String),
    Kick,
}

type ClientsChanged = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

/// Fan-out of state to every connected phone, plus a client counter.
#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<HubEvent>,
    clients: Arc<AtomicUsize>,
    on_clients_changed: ClientsChanged,
    /// Bumped on every `kick_all`, so a session that lagged past the kick event still notices it.
    kick_generation: Arc<AtomicU64>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            tx,
            clients: Arc::new(AtomicUsize::new(0)),
            on_clients_changed: Arc::new(Mutex::new(None)),
            kick_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn broadcast_state(&self, msg: &ServerMsg) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.tx.send(HubEvent::State(json));
        }
    }

    /// Closes every connected client; used after the PIN changes.
    pub fn kick_all(&self) {
        self.kick_generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.tx.send(HubEvent::Kick);
    }

    pub(crate) fn kick_generation(&self) -> u64 {
        self.kick_generation.load(Ordering::SeqCst)
    }

    pub fn client_count(&self) -> usize {
        self.clients.load(Ordering::SeqCst)
    }

    pub fn set_on_clients_changed(&self, f: Box<dyn Fn() + Send + Sync>) {
        *self.on_clients_changed.lock().unwrap() = Some(Arc::from(f));
    }

    fn subscribe(&self) -> broadcast::Receiver<HubEvent> {
        self.tx.subscribe()
    }

    pub(crate) fn client_delta(&self, delta: isize) {
        if delta > 0 {
            self.clients.fetch_add(delta as usize, Ordering::SeqCst);
        } else {
            self.clients.fetch_sub((-delta) as usize, Ordering::SeqCst);
        }
        // Clone the callback out and release the lock before calling it, so the callback may
        // take other locks (or re-enter the hub) and a panic in it cannot poison the hub.
        let callback = self.on_clients_changed.lock().unwrap().clone();
        if let Some(f) = callback {
            f();
        }
    }
}

#[derive(Clone)]
struct AppCtx {
    handler: Arc<dyn RemoteHandler>,
    hub: Hub,
    lockout: Arc<Mutex<Lockout>>,
}

pub fn router(handler: Arc<dyn RemoteHandler>, hub: Hub, lockout: Arc<Mutex<Lockout>>) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/manifest.json", get(manifest))
        .route("/icon.svg", get(icon))
        .route("/img/{id}", get(image))
        .route("/auth/status", get(auth_status))
        .route("/auth", post(auth_route))
        .route("/ws", get(ws_route))
        .with_state(AppCtx { handler, hub, lockout })
}

/// Binds `0.0.0.0:port`, falling through the next ten ports if busy. Returns the port used.
pub async fn bind(port: u16) -> std::io::Result<(TcpListener, u16)> {
    let mut last = None;
    for p in port..port.saturating_add(PORT_TRIES) {
        match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], p))).await {
            Ok(l) => return Ok((l, p)),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrInUse, "no free port")))
}

pub async fn serve(listener: TcpListener, router: Router) -> std::io::Result<()> {
    axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await
}

async fn page() -> Html<&'static str> {
    Html(PAGE_HTML)
}

async fn manifest() -> Response {
    ([(header::CONTENT_TYPE, "application/manifest+json")], MANIFEST_JSON).into_response()
}

async fn icon() -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml")], ICON_SVG).into_response()
}

async fn image(State(ctx): State<AppCtx>, AxumPath(id): AxumPath<String>) -> Response {
    let Some(path) = ctx.handler.image_path(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mime = infer::get(&bytes).map(|k| k.mime_type()).unwrap_or("application/octet-stream");
            ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache")], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize, Default)]
struct TokenQuery {
    token: Option<String>,
}

#[derive(Serialize)]
struct StatusReply {
    pin_required: bool,
    token_valid: bool,
}

async fn auth_status(State(ctx): State<AppCtx>, Query(q): Query<TokenQuery>) -> Json<StatusReply> {
    let pin_required = ctx.handler.pin_hash().is_some();
    let token_valid = q
        .token
        .as_deref()
        .map(|t| auth::verify_token(&ctx.handler.token_secret(), t))
        .unwrap_or(false);
    Json(StatusReply { pin_required, token_valid })
}

#[derive(Deserialize)]
struct AuthBody {
    pin: String,
}

#[derive(Serialize)]
struct AuthReply {
    token: String,
}

async fn auth_route(
    State(ctx): State<AppCtx>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<AuthBody>,
) -> Response {
    let ip = addr.ip();
    let now = Instant::now();
    if ctx.lockout.lock().unwrap().is_blocked(ip, now) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let issue = || Json(AuthReply { token: auth::issue_token(&ctx.handler.token_secret()) }).into_response();
    let Some(hash) = ctx.handler.pin_hash() else {
        return issue();
    };
    if auth::verify_pin(&body.pin, &hash) {
        ctx.lockout.lock().unwrap().record_success(ip);
        issue()
    } else {
        ctx.lockout.lock().unwrap().record_failure(ip, now);
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn ws_route(
    State(ctx): State<AppCtx>,
    Query(q): Query<TokenQuery>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    if ctx.handler.pin_hash().is_some() {
        let ok = q
            .token
            .as_deref()
            .map(|t| auth::verify_token(&ctx.handler.token_secret(), t))
            .unwrap_or(false);
        if !ok {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    match ws {
        Ok(ws) => ws.on_upgrade(move |socket| client_session(socket, ctx)),
        Err(rejection) => rejection.into_response(),
    }
}

async fn client_session(socket: WebSocket, ctx: AppCtx) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = ctx.hub.subscribe();
    let connected_generation = ctx.hub.kick_generation();
    ctx.hub.client_delta(1);

    let json = serde_json::to_string(&ctx.handler.snapshot()).unwrap_or_default();
    if sink.send(Message::Text(json.into())).await.is_err() {
        ctx.hub.client_delta(-1);
        return;
    }

    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(HubEvent::State(json)) => {
                    if sink.send(Message::Text(json.into())).await.is_err() { break; }
                }
                Ok(HubEvent::Kick) => {
                    let _ = sink.send(Message::Close(None)).await;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // The lag may have dropped a Kick; the generation tells us whether one happened.
                    if ctx.hub.kick_generation() != connected_generation {
                        let _ = sink.send(Message::Close(None)).await;
                        break;
                    }
                    let json = serde_json::to_string(&ctx.handler.snapshot()).unwrap_or_default();
                    if sink.send(Message::Text(json.into())).await.is_err() { break; }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(text))) => handle_client_msg(&ctx, &text),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            }
        }
    }
    ctx.hub.client_delta(-1);
}

fn handle_client_msg(ctx: &AppCtx, text: &str) {
    match serde_json::from_str::<ClientMsg>(text) {
        Ok(ClientMsg::Play { id }) => ctx.handler.play(&id),
        Ok(ClientMsg::StopAll) => ctx.handler.stop_all(),
        Ok(ClientMsg::SetMasterVolume { value }) => ctx.handler.set_master_volume(value.clamp(0.0, 1.0)),
        Err(e) => log::debug!("ignoring remote message {text:?}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_broadcast_without_subscribers_is_fine() {
        let hub = Hub::new();
        hub.broadcast_state(&ServerMsg::State { master_volume: 1.0, playing: vec![], sounds: vec![] });
        hub.kick_all();
        assert_eq!(hub.client_count(), 0);
    }

    #[test]
    fn kick_all_increments_generation() {
        let hub = Hub::new();
        assert_eq!(hub.kick_generation(), 0);
        hub.kick_all();
        hub.kick_all();
        assert_eq!(hub.kick_generation(), 2);
    }

    #[test]
    fn clients_changed_callback_runs_without_hub_lock() {
        let hub = Hub::new();
        let inner = hub.clone();
        hub.set_on_clients_changed(Box::new(move || {
            // Re-entrant: would deadlock if the hub mutex were held during the callback.
            inner.set_on_clients_changed(Box::new(|| {}));
        }));
        hub.client_delta(1);
        assert_eq!(hub.client_count(), 1);
    }

    #[tokio::test]
    async fn hub_subscriber_receives_state_and_kick() {
        let hub = Hub::new();
        let mut rx = hub.subscribe();
        hub.broadcast_state(&ServerMsg::State { master_volume: 1.0, playing: vec![], sounds: vec![] });
        hub.kick_all();
        assert!(matches!(rx.recv().await.unwrap(), HubEvent::State(_)));
        assert!(matches!(rx.recv().await.unwrap(), HubEvent::Kick));
    }

    #[tokio::test]
    async fn bind_skips_busy_port() {
        let (first, p1) = bind(39000).await.unwrap();
        let (_second, p2) = bind(39000).await.unwrap();
        assert_eq!(p1, 39000);
        assert_eq!(p2, 39001);
        drop(first);
    }
}
