//! Serves the phone page with fake sounds on port 7331 (or the next free one).
//! cargo run -p soundboard-core --example remote_dev          -> no PIN
//! cargo run -p soundboard-core --example remote_dev 1234     -> PIN 1234
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use soundboard_core::palette::PALETTE;
use soundboard_core::remote::auth::{hash_pin, Lockout};
use soundboard_core::remote::protocol::{RemoteSound, ServerMsg};
use soundboard_core::remote::{bind, router, serve, Hub, RemoteHandler};

struct Dev {
    playing: Arc<Mutex<Vec<String>>>,
    seqs: Arc<Mutex<HashMap<String, u64>>>,
    hub: Hub,
    pin_hash: Option<String>,
}

fn snapshot(playing: &[String], seqs: &HashMap<String, u64>) -> ServerMsg {
    ServerMsg::State {
        master_volume: 0.8,
        playing: playing.to_vec(),
        sounds: (1..=7)
            .map(|i| RemoteSound {
                id: format!("s{i}"),
                name: format!("Sound {i}"),
                volume: 1.0,
                has_image: false,
                color: PALETTE[i % PALETTE.len()].into(),
                // Sound 7 plays with an unknown length to exercise the indeterminate bar.
                duration_secs: if i == 7 { None } else { Some(1.0) },
                play_seq: seqs.get(&format!("s{i}")).copied().unwrap_or(0),
            })
            .collect(),
    }
}

impl RemoteHandler for Dev {
    fn snapshot(&self) -> ServerMsg {
        snapshot(&self.playing.lock().unwrap(), &self.seqs.lock().unwrap())
    }
    fn play(&self, id: &str) {
        println!("play {id}");
        self.playing.lock().unwrap().push(id.to_string());
        *self.seqs.lock().unwrap().entry(id.to_string()).or_insert(0) += 1;
        self.hub.broadcast_state(&self.snapshot());
        let (playing, seqs, hub, id) = (self.playing.clone(), self.seqs.clone(), self.hub.clone(), id.to_string());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let snap = {
                let mut p = playing.lock().unwrap();
                if let Some(i) = p.iter().position(|x| *x == id) {
                    p.remove(i);
                }
                snapshot(&p, &seqs.lock().unwrap())
            };
            hub.broadcast_state(&snap);
        });
    }
    fn stop_all(&self) {
        println!("stop all");
        self.playing.lock().unwrap().clear();
        self.hub.broadcast_state(&self.snapshot());
    }
    fn set_master_volume(&self, value: f32) {
        println!("master {value}");
    }
    fn image_path(&self, _id: &str) -> Option<PathBuf> {
        None
    }
    fn pin_hash(&self) -> Option<String> {
        self.pin_hash.clone()
    }
    fn token_secret(&self) -> String {
        "dev-secret".into()
    }
}

#[tokio::main]
async fn main() {
    let hub = Hub::new();
    let dev = Arc::new(Dev {
        playing: Arc::new(Mutex::new(vec![])),
        seqs: Arc::new(Mutex::new(HashMap::new())),
        hub: hub.clone(),
        pin_hash: std::env::args().nth(1).map(|p| hash_pin(&p)),
    });
    let app = router(dev, hub, Arc::new(Mutex::new(Lockout::standard())));
    let (listener, port) = bind(7331).await.expect("bind");
    println!("open http://localhost:{port}  (phone: http://<your LAN ip>:{port})");
    serve(listener, app).await.expect("serve");
}
