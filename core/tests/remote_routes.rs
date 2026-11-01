use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use soundboard_core::remote::auth::{self, Lockout};
use soundboard_core::remote::protocol::{RemoteSound, ServerMsg};
use soundboard_core::remote::{router, Hub, RemoteHandler};
use tower::ServiceExt;

struct FakeHandler {
    pin_hash: Option<String>,
    secret: String,
    calls: Mutex<Vec<String>>,
}

impl RemoteHandler for FakeHandler {
    fn snapshot(&self) -> ServerMsg {
        ServerMsg::State {
            master_volume: 0.8,
            playing: vec![],
            sounds: vec![RemoteSound { id: "u1".into(), name: "Airhorn".into(), volume: 1.0, has_image: false, color: "#FF595E".into() }],
        }
    }
    fn play(&self, id: &str) {
        self.calls.lock().unwrap().push(format!("play:{id}"));
    }
    fn stop_all(&self) {
        self.calls.lock().unwrap().push("stop_all".into());
    }
    fn set_master_volume(&self, value: f32) {
        self.calls.lock().unwrap().push(format!("master:{value}"));
    }
    fn image_path(&self, _id: &str) -> Option<PathBuf> {
        None
    }
    fn pin_hash(&self) -> Option<String> {
        self.pin_hash.clone()
    }
    fn token_secret(&self) -> String {
        self.secret.clone()
    }
}

fn app(pin: Option<&str>) -> (axum::Router, Arc<FakeHandler>) {
    let h = Arc::new(FakeHandler {
        pin_hash: pin.map(auth::hash_pin),
        secret: "test-secret".into(),
        calls: Mutex::new(vec![]),
    });
    let r = router(h.clone(), Hub::new(), Arc::new(Mutex::new(Lockout::standard())));
    (r, h)
}

fn req(method: &str, uri: &str, body: &str) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    r.extensions_mut().insert(ConnectInfo(SocketAddr::from(([192, 168, 1, 50], 40000))));
    r
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn root_serves_page_and_static_files() {
    let (app, _) = app(None);
    let r = app.clone().oneshot(req("GET", "/", "")).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(body_string(r).await.contains("<html"));
    let r = app.clone().oneshot(req("GET", "/manifest.json", "")).await.unwrap();
    assert_eq!(r.headers()["content-type"], "application/manifest+json");
    let r = app.oneshot(req("GET", "/icon.svg", "")).await.unwrap();
    assert_eq!(r.headers()["content-type"], "image/svg+xml");
}

#[tokio::test]
async fn auth_status_reports_pin_and_token() {
    let (app, _) = app(Some("1234"));
    let r = app.clone().oneshot(req("GET", "/auth/status", "")).await.unwrap();
    assert_eq!(body_string(r).await, r#"{"pin_required":true,"token_valid":false}"#);
    let token = auth::issue_token("test-secret");
    let r = app.oneshot(req("GET", &format!("/auth/status?token={token}"), "")).await.unwrap();
    assert_eq!(body_string(r).await, r#"{"pin_required":true,"token_valid":true}"#);

    let (app, _) = self::app(None);
    let r = app.oneshot(req("GET", "/auth/status", "")).await.unwrap();
    assert_eq!(body_string(r).await, r#"{"pin_required":false,"token_valid":false}"#);
}

#[tokio::test]
async fn auth_wrong_pin_is_401_and_right_pin_returns_token() {
    let (app, _) = app(Some("1234"));
    let r = app.clone().oneshot(req("POST", "/auth", r#"{"pin":"0000"}"#)).await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    let r = app.oneshot(req("POST", "/auth", r#"{"pin":"1234"}"#)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body_string(r).await).unwrap();
    assert!(auth::verify_token("test-secret", json["token"].as_str().unwrap()));
}

#[tokio::test]
async fn sixth_wrong_attempt_is_429() {
    let (app, _) = app(Some("1234"));
    for _ in 0..5 {
        let r = app.clone().oneshot(req("POST", "/auth", r#"{"pin":"0000"}"#)).await.unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }
    let r = app.oneshot(req("POST", "/auth", r#"{"pin":"1234"}"#)).await.unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS, "even the right pin is blocked");
}

#[tokio::test]
async fn ws_without_token_when_pin_set_is_401() {
    let (app, _) = app(Some("1234"));
    let r = app.oneshot(req("GET", "/ws", "")).await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_passes_auth_without_pin_then_fails_upgrade_with_400() {
    let (app, _) = app(None);
    let r = app.oneshot(req("GET", "/ws", "")).await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST, "no Upgrade header in a plain request");
}

#[tokio::test]
async fn ws_with_valid_token_passes_auth() {
    let (app, _) = app(Some("1234"));
    let token = auth::issue_token("test-secret");
    let r = app.oneshot(req("GET", &format!("/ws?token={token}"), "")).await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST, "auth ok, upgrade headers missing");
}

#[tokio::test]
async fn unknown_image_is_404() {
    let (app, _) = app(None);
    let r = app.oneshot(req("GET", "/img/nope", "")).await.unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}
