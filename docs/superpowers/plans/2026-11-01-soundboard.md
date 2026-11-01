# Soundboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows desktop soundboard (Tauri 2 + React) that plays local audio files to speakers and a virtual mic at once, triggered by tiles, global hotkeys, or a phone web page on the LAN.

**Architecture:** A Tauri-free Rust crate `soundboard-core` holds all logic (config, audio engine with a swappable backend, library, hotkey rules, axum remote server). The Tauri app crate `src-tauri` wires core to commands, events, the global-shortcut plugin and the remote server lifecycle. The React UI and the bundled phone page only render state and send commands; every trigger ends in one Rust `play(sound_id)`.

**Tech Stack:** Rust 2021, Tauri 2, rodio 0.20 (cpal WASAPI), axum 0.8, tokio, argon2, hmac/sha2, React 19, TypeScript, Vite 6, Tailwind 4, zustand 5, dnd-kit, qrcode.react, vitest + Testing Library.

Spec: `docs/superpowers/specs/2026-11-01-soundboard-design.md`. Read it before starting any task.

## Global Constraints

- Platform: Windows 11, PowerShell shell. Run commands from the repo root `F:\Claude\Soundboard` unless a step says otherwise.
- Node 24 / npm 11 / Rust 1.97 are installed. Tauri CLI comes from the npm dev dependency `@tauri-apps/cli`; run it as `npx tauri ...` or `npm run tauri -- ...`.
- Crate names: `soundboard-core` (lib `soundboard_core`, path `core/`), `soundboard` (lib `soundboard_lib`, path `src-tauri/`). Workspace root `Cargo.toml` lists both.
- Core crate must not depend on `tauri`. All unit tests live in core and run with `cargo test -p soundboard-core`.
- Config schema exactly as in the spec; `version` is `1`; default port `7331`; default Stop All hotkey `Ctrl+Shift+F12`.
- Hotkey accelerator format: modifiers in fixed order `Ctrl`, `Alt`, `Shift`, `Super`, then one `KeyboardEvent.code` name, joined by `+`. Example `Ctrl+Shift+Digit1`.
- Supported audio extensions: `mp3`, `wav`, `ogg`, `flac`. Supported image MIME types: `image/png`, `image/jpeg`, `image/webp`, `image/gif`.
- Decoded buffer cache cap: 200 MB. Reaper interval: 100 ms. Port retry: the requested port plus the next 10.
- PIN: 4 to 8 ASCII digits. Lockout: 5 failures per IP in 10 minutes blocks for 60 seconds (HTTP 429).
- UI tokens (hex, exact): bg `#0A0A0B`, surface `#141416`, surface-2 `#1C1C1F`, border `#26262A`, text `#EDEDEF`, dim `#8A8A93`, mute `#55555C`, accent `#F5B324`, accent-ink `#1A1200`, danger `#EF4444`, radius `8px`. Accent means live/focus/primary; danger means stop/delete. Nothing else is colored. No decorative icons, no headers over obvious groups, panels not modals (except Delete confirm).
- Commit after every task with a conventional-commit message. Git identity is already configured in the repo.
- Every Tauri command returns `Result<T, String>` when it can fail; the string is shown to the user as a toast, so write it as a sentence for a human.
- Never hold the `App` mutex while calling `emit_state` (it locks the same mutex). Lock, mutate, drop the guard, then emit.

---

## File structure

```
Soundboard/
├─ Cargo.toml                       workspace
├─ package.json, vite.config.ts, tsconfig.json, index.html, app-icon.svg
├─ core/
│  ├─ Cargo.toml
│  ├─ src/lib.rs                    module list + #[cfg(test)] test_util
│  ├─ src/test_util.rs              write_test_wav helper (tests only)
│  ├─ src/config.rs                 Config types, load/save
│  ├─ src/library.rs                import/delete sounds + images
│  ├─ src/hotkeys.rs                normalize, conflicts, Owner
│  ├─ src/audio/mod.rs              re-exports
│  ├─ src/audio/backend.rs          AudioBackend trait + value types
│  ├─ src/audio/cache.rs            BufferCache (LRU)
│  ├─ src/audio/fake_backend.rs     FakeBackend for tests
│  ├─ src/audio/decode.rs           decode_file, extension check
│  ├─ src/audio/engine.rs           Engine<B>, Outputs, PlayOutcome, sine
│  ├─ src/audio/rodio_backend.rs    RodioBackend (audio thread)
│  ├─ src/remote/mod.rs             router, Hub, RemoteHandler, bind, serve
│  ├─ src/remote/auth.rs            PIN hash, tokens, Lockout
│  ├─ src/remote/protocol.rs        ServerMsg, ClientMsg, RemoteSound
│  ├─ src/remote/page.html          phone page
│  ├─ src/remote/manifest.json, icon.svg
│  ├─ examples/tone.rs              manual device check
│  ├─ examples/remote_dev.rs        run phone page without Tauri
│  └─ tests/remote_routes.rs        axum integration tests
├─ src-tauri/
│  ├─ Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
│  └─ src/{main.rs, lib.rs, state.rs, commands.rs, hotkeys_glue.rs, remote_glue.rs}
└─ src/
   ├─ main.tsx, App.tsx, index.css, test-setup.ts
   ├─ lib/{api.ts, accel.ts, cx.ts, store.ts, actions.ts, validate.ts}
   ├─ lib/{accel.test.ts, store.test.ts, validate.test.ts}
   └─ components/{TopBar, Toasts, TileGrid, Tile, ContextMenu, Dialogs, HotkeyCapture, EmptyState, Banner, SettingsPanel, RemotePanel}.tsx (+ .test.tsx)
```

---

### Task 1: Scaffold workspace, Tauri app shell, React + Tailwind + vitest

**Files:**
- Create: `Cargo.toml`, `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `app-icon.svg`, `.gitignore` (modify)
- Create: `core/Cargo.toml`, `core/src/lib.rs`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `src/main.tsx`, `src/App.tsx`, `src/index.css`, `src/test-setup.ts`

**Interfaces:**
- Produces: a workspace where `cargo check --workspace`, `npm run build`, `npm test` all pass and `npm run tauri dev` opens a window.

- [ ] **Step 1: Workspace and gitignore**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["core", "src-tauri"]
```

Append to `.gitignore`:
```
src-tauri/gen/schemas/
target/
```

- [ ] **Step 2: Core crate skeleton with all dependencies**

`core/Cargo.toml`:
```toml
[package]
name = "soundboard-core"
version = "0.1.0"
edition = "2021"

[lib]
name = "soundboard_core"
path = "src/lib.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
thiserror = "2"
log = "0.4"
rodio = "0.20"
argon2 = "0.5"
hmac = "0.12"
sha2 = "0.10"
base64 = "0.22"
rand = "0.8"
infer = "0.16"
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["full"] }
futures-util = "0.3"

[dev-dependencies]
tempfile = "3"
hound = "3"
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

`core/src/lib.rs`:
```rust
//! Soundboard core: everything that works without Tauri.
```

- [ ] **Step 3: Tauri app crate**

`src-tauri/Cargo.toml`:
```toml
[package]
name = "soundboard"
version = "0.1.0"
edition = "2021"

[lib]
name = "soundboard_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
soundboard-core = { path = "../core" }
tauri = { version = "2", features = ["protocol-asset"] }
tauri-plugin-dialog = "2"
tauri-plugin-global-shortcut = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
log = "0.4"
env_logger = "0.11"
local-ip-address = "0.6"
tokio = { version = "1", features = ["full"] }
```

`src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    soundboard_lib::run();
}
```

`src-tauri/src/lib.rs`:
```rust
pub fn run() {
    env_logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

`src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Soundboard",
  "version": "0.1.0",
  "identifier": "co.carbonweb.soundboard",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Soundboard",
        "width": 960,
        "height": 640,
        "minWidth": 520,
        "minHeight": 360,
        "dragDropEnabled": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost data:; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost",
      "assetProtocol": {
        "enable": true,
        "scope": ["$APPDATA/images/**"]
      }
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.ico"
    ]
  }
}
```

`src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:default"]
}
```

- [ ] **Step 4: App icon**

`app-icon.svg` (repo root):
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="224" fill="#141416"/>
  <rect x="224" y="416" width="128" height="192" rx="32" fill="#ededef"/>
  <rect x="448" y="288" width="128" height="448" rx="32" fill="#f5b324"/>
  <rect x="672" y="352" width="128" height="320" rx="32" fill="#ededef"/>
</svg>
```

- [ ] **Step 5: Frontend tooling**

`package.json`:
```json
{
  "name": "soundboard",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "@dnd-kit/core": "^6.3.1",
    "@dnd-kit/sortable": "^10.0.0",
    "@dnd-kit/utilities": "^3.2.2",
    "@tauri-apps/api": "^2.2.0",
    "@tauri-apps/plugin-dialog": "^2.2.0",
    "qrcode.react": "^4.2.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "zustand": "^5.0.3"
  },
  "devDependencies": {
    "@tailwindcss/vite": "^4.0.0",
    "@tauri-apps/cli": "^2.2.0",
    "@testing-library/jest-dom": "^6.6.3",
    "@testing-library/react": "^16.2.0",
    "@testing-library/user-event": "^14.6.1",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.4",
    "jsdom": "^26.0.0",
    "tailwindcss": "^4.0.0",
    "typescript": "~5.7.2",
    "vite": "^6.0.0",
    "vitest": "^3.0.0"
  }
}
```

`vite.config.ts`:
```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: { ignored: ["**/src-tauri/**", "**/core/**", "**/target/**"] },
  },
  test: {
    environment: "jsdom",
    globals: true,
    passWithNoTests: true,
    setupFiles: ["./src/test-setup.ts"],
  },
});
```

`tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "resolveJsonModule": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src", "vite.config.ts"]
}
```

`index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Soundboard</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/test-setup.ts`:
```ts
import "@testing-library/jest-dom/vitest";
```

`src/main.tsx`:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`src/App.tsx` (shell only, replaced in Task 13):
```tsx
export default function App() {
  return <div className="flex h-full items-center justify-center text-dim">Soundboard</div>;
}
```

`src/index.css`:
```css
@import "tailwindcss";

@theme {
  --color-bg: #0a0a0b;
  --color-surface: #141416;
  --color-surface-2: #1c1c1f;
  --color-border: #26262a;
  --color-text: #ededef;
  --color-dim: #8a8a93;
  --color-mute: #55555c;
  --color-accent: #f5b324;
  --color-accent-ink: #1a1200;
  --color-danger: #ef4444;
  --radius-md: 8px;
  --font-sans: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, "Cascadia Mono", Consolas, monospace;
}

html, body, #root { height: 100%; }

body {
  background: var(--color-bg);
  color: var(--color-text);
  font-family: var(--font-sans);
  font-size: 13px;
  line-height: 1.3;
  user-select: none;
  -webkit-user-select: none;
  overflow: hidden;
}

:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }

input[type="range"] {
  appearance: none;
  height: 16px;
  background: transparent;
  cursor: pointer;
}
input[type="range"]::-webkit-slider-runnable-track { height: 2px; background: var(--color-border); }
input[type="range"]::-webkit-slider-thumb {
  appearance: none;
  width: 10px;
  height: 10px;
  margin-top: -4px;
  border-radius: 50%;
  background: var(--color-text);
}

@layer components {
  .btn {
    @apply h-8 rounded-md border border-border bg-surface px-3 text-text hover:bg-surface-2 disabled:opacity-40 disabled:hover:bg-surface aria-pressed:border-dim aria-pressed:bg-surface-2;
  }
  .field {
    @apply h-8 w-full rounded-md border border-border bg-bg px-2 text-text;
  }
  .panel { @apply w-80 shrink-0 overflow-y-auto border-l border-border p-4; }
  .label { @apply mb-1 block text-xs text-dim; }
}
```

- [ ] **Step 6: Install and generate icons**

Run:
```bash
npm install
```
```bash
npx tauri icon app-icon.svg
```
Expected: `src-tauri/icons/` now contains `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.png` and others.

- [ ] **Step 7: Verify everything compiles and tests run**

Run:
```bash
cargo check --workspace
```
Expected: `Finished` with no errors (first run downloads crates, several minutes).

```bash
npm run build
```
Expected: `dist/` produced, no TypeScript errors.

```bash
npm test
```
Expected: vitest reports no test files and exits 0.

- [ ] **Step 8: Smoke-run the window**

Run in the background (takes a few minutes the first time):
```bash
npm run tauri dev
```
Expected: a 960×640 window titled "Soundboard" with the word "Soundboard" centered in gray on a near-black background. Stop the process after confirming.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "chore: scaffold Tauri workspace, core crate, React + Tailwind + vitest"
```

---

### Task 2: Config module

**Files:**
- Create: `core/src/config.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct Config { version: u32, master_volume: f32, devices: Devices, remote: RemoteConfig, stop_all_hotkey: String, sounds: Vec<Sound> }` with `Default`.
  - `pub struct Sound { id: String, name: String, file: String, image: Option<String>, volume: f32, hotkey: Option<String> }`
  - `pub struct Devices { speakers: Option<String>, virtual_mic: Option<String> }`
  - `pub struct RemoteConfig { port: u16, pin_hash: Option<String>, token_secret: String }`
  - `Config::sound(&self, id) -> Option<&Sound>`, `Config::sound_mut(&mut self, id) -> Option<&mut Sound>`
  - `pub fn new_secret() -> String`
  - `pub enum LoadOutcome { Loaded(Config), Fresh(Config), Recovered { config: Config, backup: PathBuf } }` with `into_config()`
  - `pub fn load(path: &Path) -> LoadOutcome`, `pub fn save(path: &Path, cfg: &Config) -> io::Result<()>`
  - Consts `CONFIG_VERSION = 1`, `DEFAULT_PORT = 7331`, `DEFAULT_STOP_ALL_HOTKEY = "Ctrl+Shift+F12"`

- [ ] **Step 1: Write failing tests**

`core/src/config.rs` (tests first; implementation below in the same file):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_has_expected_values() {
        let c = Config::default();
        assert_eq!(c.version, CONFIG_VERSION);
        assert_eq!(c.master_volume, 0.8);
        assert_eq!(c.remote.port, DEFAULT_PORT);
        assert_eq!(c.remote.pin_hash, None);
        assert_eq!(c.stop_all_hotkey, DEFAULT_STOP_ALL_HOTKEY);
        assert!(c.sounds.is_empty());
        assert_eq!(c.remote.token_secret.len(), 44, "32 random bytes base64");
    }

    #[test]
    fn missing_file_is_fresh() {
        let dir = tempdir().unwrap();
        match load(&dir.path().join("config.json")) {
            LoadOutcome::Fresh(_) => {}
            other => panic!("expected Fresh, got {other:?}"),
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = Config::default();
        cfg.sounds.push(Sound {
            id: "a".into(),
            name: "Air".into(),
            file: "air.mp3".into(),
            image: Some("a.png".into()),
            volume: 0.5,
            hotkey: Some("Ctrl+Shift+Digit1".into()),
        });
        save(&path, &cfg).unwrap();
        match load(&path) {
            LoadOutcome::Loaded(loaded) => assert_eq!(loaded, cfg),
            other => panic!("expected Loaded, got {other:?}"),
        }
        assert!(!path.with_extension("json.tmp").exists(), "temp file cleaned up");
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaulted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not json").unwrap();
        match load(&path) {
            LoadOutcome::Recovered { config, backup } => {
                assert!(config.sounds.is_empty());
                assert_eq!(fs::read_to_string(backup).unwrap(), "{ not json");
            }
            other => panic!("expected Recovered, got {other:?}"),
        }
    }

    #[test]
    fn sound_missing_optional_fields_gets_defaults() {
        let json = r#"{"version":1,"master_volume":1.0,"sounds":[{"id":"x","name":"X","file":"x.wav"}]}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.sounds[0].volume, 1.0);
        assert_eq!(cfg.sounds[0].hotkey, None);
        assert_eq!(cfg.sounds[0].image, None);
        assert_eq!(cfg.remote.port, DEFAULT_PORT);
        assert_eq!(cfg.stop_all_hotkey, DEFAULT_STOP_ALL_HOTKEY);
    }

    #[test]
    fn sound_lookup_by_id() {
        let mut cfg = Config::default();
        cfg.sounds.push(Sound { id: "a".into(), name: "A".into(), file: "a.wav".into(), image: None, volume: 1.0, hotkey: None });
        assert_eq!(cfg.sound("a").unwrap().name, "A");
        assert!(cfg.sound("zzz").is_none());
        cfg.sound_mut("a").unwrap().name = "B".into();
        assert_eq!(cfg.sound("a").unwrap().name, "B");
    }
}
```

Add to `core/src/lib.rs`:
```rust
pub mod config;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core config`
Expected: compile error, `Config` not found.

- [ ] **Step 3: Implement**

Top of `core/src/config.rs` (above the tests module):
```rust
//! Persistent configuration: `config.json` in the app data directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;
pub const DEFAULT_PORT: u16 = 7331;
pub const DEFAULT_STOP_ALL_HOTKEY: &str = "Ctrl+Shift+F12";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    pub id: String,
    pub name: String,
    /// File name relative to `<app_data>/sounds/`.
    pub file: String,
    /// File name relative to `<app_data>/images/`, or None.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default = "one")]
    pub volume: f32,
    #[serde(default)]
    pub hotkey: Option<String>,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Devices {
    #[serde(default)]
    pub speakers: Option<String>,
    #[serde(default)]
    pub virtual_mic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub port: u16,
    #[serde(default)]
    pub pin_hash: Option<String>,
    #[serde(default = "new_secret")]
    pub token_secret: String,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self { port: DEFAULT_PORT, pin_hash: None, token_secret: new_secret() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub master_volume: f32,
    #[serde(default)]
    pub devices: Devices,
    #[serde(default)]
    pub remote: RemoteConfig,
    #[serde(default = "default_stop_all")]
    pub stop_all_hotkey: String,
    #[serde(default)]
    pub sounds: Vec<Sound>,
}

fn default_stop_all() -> String {
    DEFAULT_STOP_ALL_HOTKEY.to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            master_volume: 0.8,
            devices: Devices::default(),
            remote: RemoteConfig::default(),
            stop_all_hotkey: default_stop_all(),
            sounds: Vec::new(),
        }
    }
}

impl Config {
    pub fn sound(&self, id: &str) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.id == id)
    }

    pub fn sound_mut(&mut self, id: &str) -> Option<&mut Sound> {
        self.sounds.iter_mut().find(|s| s.id == id)
    }
}

/// 32 random bytes, standard base64 (44 chars).
pub fn new_secret() -> String {
    use base64::Engine;
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[derive(Debug)]
pub enum LoadOutcome {
    Loaded(Config),
    Fresh(Config),
    Recovered { config: Config, backup: PathBuf },
}

impl LoadOutcome {
    pub fn into_config(self) -> Config {
        match self {
            LoadOutcome::Loaded(c) | LoadOutcome::Fresh(c) => c,
            LoadOutcome::Recovered { config, .. } => config,
        }
    }
}

pub fn load(path: &Path) -> LoadOutcome {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return LoadOutcome::Fresh(Config::default()),
        Err(e) => {
            log::warn!("config read failed: {e}");
            return LoadOutcome::Fresh(Config::default());
        }
    };
    match serde_json::from_str::<Config>(&text) {
        Ok(cfg) => LoadOutcome::Loaded(cfg),
        Err(e) => {
            log::warn!("config parse failed, starting fresh: {e}");
            let backup = path.with_extension("json.bak");
            if let Err(e) = fs::copy(path, &backup) {
                log::warn!("could not back up corrupt config: {e}");
            }
            LoadOutcome::Recovered { config: Config::default(), backup }
        }
    }
}

/// Atomic save: write to `config.json.tmp` then rename over the target.
pub fn save(path: &Path, cfg: &Config) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(cfg).map_err(io::Error::other)?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p soundboard-core config`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add core/src/config.rs core/src/lib.rs
git commit -m "feat(core): config types with atomic save and corrupt-file recovery"
```

---

### Task 3: Audio backend trait, buffer cache, fake backend

**Files:**
- Create: `core/src/audio/mod.rs`, `core/src/audio/backend.rs`, `core/src/audio/cache.rs`, `core/src/audio/fake_backend.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct SinkHandle(pub u64)` (Copy, Eq, Hash)
  - `pub enum DeviceTarget { Default, Named(String) }` with `DeviceTarget::from_name(Option<&str>) -> Self`
  - `pub struct DeviceInfo { name: String, is_default: bool }` (Serialize)
  - `pub struct DecodedBuffer { channels: u16, sample_rate: u32, samples: Arc<Vec<f32>> }` with `new`, `byte_len()`, `duration_secs()`
  - `pub enum AudioError { DeviceNotFound(String), Stream(String), Decode(String), NoOutputs, Io(io::Error) }`
  - `pub trait AudioBackend: Send + Sync { devices, play(&DeviceTarget, &DecodedBuffer, gain) -> Result<SinkHandle, AudioError>, stop, set_gain, is_finished, reset }`
  - `pub struct BufferCache` with `new(max_bytes)`, `get(&mut self, id) -> Option<DecodedBuffer>`, `insert(&mut self, id, buf)`, `remove`, `len`, `is_empty`, `total_bytes`
  - `pub struct FakeBackend` with `new(&[&str])`, `calls() -> Vec<Call>`, `handles() -> Vec<SinkHandle>`, `mark_finished(SinkHandle)`, `gain(SinkHandle) -> Option<f32>`; `pub enum Call { Play { handle, target, gain, samples }, Stop(SinkHandle), SetGain(SinkHandle, f32), Reset }`

- [ ] **Step 1: Write failing tests**

`core/src/audio/cache.rs` tests (place at bottom of file):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn buf(n: usize) -> DecodedBuffer {
        DecodedBuffer::new(1, 44100, vec![0.5; n])
    }

    #[test]
    fn insert_and_get_returns_buffer() {
        let mut c = BufferCache::new(1024);
        c.insert("a", buf(4));
        let got = c.get("a").unwrap();
        assert_eq!(got.samples.len(), 4);
        assert_eq!(c.total_bytes(), 16);
    }

    #[test]
    fn evicts_least_recently_used_when_over_cap() {
        // cap = 16 bytes = four f32 samples; each buffer is 2 samples = 8 bytes
        let mut c = BufferCache::new(16);
        c.insert("a", buf(2));
        c.insert("b", buf(2));
        assert!(c.get("a").is_some(), "touch a so b is the oldest");
        c.insert("c", buf(2));
        assert!(c.get("b").is_none(), "b evicted");
        assert!(c.get("a").is_some());
        assert!(c.get("c").is_some());
        assert_eq!(c.len(), 2);
        assert_eq!(c.total_bytes(), 16);
    }

    #[test]
    fn remove_frees_bytes() {
        let mut c = BufferCache::new(1024);
        c.insert("a", buf(4));
        c.remove("a");
        assert!(c.is_empty());
        assert_eq!(c.total_bytes(), 0);
    }

    #[test]
    fn single_oversized_entry_is_kept() {
        let mut c = BufferCache::new(8);
        c.insert("big", buf(100));
        assert!(c.get("big").is_some());
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn reinserting_same_id_replaces() {
        let mut c = BufferCache::new(1024);
        c.insert("a", buf(2));
        c.insert("a", buf(6));
        assert_eq!(c.len(), 1);
        assert_eq!(c.total_bytes(), 24);
    }
}
```

`core/src/audio/fake_backend.rs` tests (bottom of file):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_rejects_unknown_named_device() {
        let f = FakeBackend::new(&["Speakers"]);
        let b = DecodedBuffer::new(1, 44100, vec![0.0; 10]);
        let err = f.play(&DeviceTarget::Named("Ghost".into()), &b, 1.0).unwrap_err();
        assert!(matches!(err, AudioError::DeviceNotFound(n) if n == "Ghost"));
    }

    #[test]
    fn fake_records_calls_and_handles() {
        let f = FakeBackend::new(&["Speakers"]);
        let b = DecodedBuffer::new(1, 44100, vec![0.0; 10]);
        let h = f.play(&DeviceTarget::Default, &b, 0.5).unwrap();
        f.set_gain(h, 0.25);
        assert_eq!(f.gain(h), Some(0.25));
        assert!(!f.is_finished(h));
        f.mark_finished(h);
        assert!(f.is_finished(h));
        f.stop(h);
        assert_eq!(f.handles(), vec![h]);
        assert_eq!(f.calls().len(), 3);
        assert!(f.devices()[0].is_default);
    }
}
```

`core/src/audio/mod.rs`:
```rust
pub mod backend;
pub mod cache;
pub mod fake_backend;

pub use backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};
pub use cache::BufferCache;
pub use fake_backend::FakeBackend;
```

Add to `core/src/lib.rs`:
```rust
pub mod audio;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core audio`
Expected: compile errors (types missing).

- [ ] **Step 3: Implement backend.rs**

`core/src/audio/backend.rs`:
```rust
use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;

/// Opaque id for one playing sink on the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SinkHandle(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceTarget {
    Default,
    Named(String),
}

impl DeviceTarget {
    pub fn from_name(name: Option<&str>) -> Self {
        match name {
            Some(n) if !n.is_empty() => DeviceTarget::Named(n.to_string()),
            _ => DeviceTarget::Default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// Interleaved f32 PCM, shared so the cache and active sinks never copy.
#[derive(Debug, Clone)]
pub struct DecodedBuffer {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Arc<Vec<f32>>,
}

impl DecodedBuffer {
    pub fn new(channels: u16, sample_rate: u32, samples: Vec<f32>) -> Self {
        Self { channels, sample_rate, samples: Arc::new(samples) }
    }

    pub fn byte_len(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / (self.channels.max(1) as f32 * self.sample_rate.max(1) as f32)
    }
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("output device not found: {0}")]
    DeviceNotFound(String),
    #[error("audio stream error: {0}")]
    Stream(String),
    #[error("could not decode {0}")]
    Decode(String),
    #[error("no output device selected")]
    NoOutputs,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub trait AudioBackend: Send + Sync {
    fn devices(&self) -> Vec<DeviceInfo>;
    /// Start playing `buffer` on `target` at linear `gain`. Returns a handle for later control.
    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError>;
    fn stop(&self, handle: SinkHandle);
    fn set_gain(&self, handle: SinkHandle, gain: f32);
    /// True once the sink has drained (or never existed).
    fn is_finished(&self, handle: SinkHandle) -> bool;
    /// Drop every open device stream; next play reopens lazily.
    fn reset(&self);
}
```

- [ ] **Step 4: Implement cache.rs**

Top of `core/src/audio/cache.rs`:
```rust
use std::collections::{HashMap, VecDeque};

use super::backend::DecodedBuffer;

/// LRU cache of decoded buffers keyed by sound id, bounded by total bytes.
/// Always keeps at least one entry so a single oversized clip still plays.
pub struct BufferCache {
    max_bytes: usize,
    total: usize,
    entries: HashMap<String, DecodedBuffer>,
    order: VecDeque<String>,
}

impl BufferCache {
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes, total: 0, entries: HashMap::new(), order: VecDeque::new() }
    }

    pub fn get(&mut self, id: &str) -> Option<DecodedBuffer> {
        let buf = self.entries.get(id)?.clone();
        self.touch(id);
        Some(buf)
    }

    pub fn insert(&mut self, id: &str, buf: DecodedBuffer) {
        self.remove(id);
        self.total += buf.byte_len();
        self.entries.insert(id.to_string(), buf);
        self.order.push_back(id.to_string());
        while self.total > self.max_bytes && self.order.len() > 1 {
            if let Some(old) = self.order.pop_front() {
                if let Some(b) = self.entries.remove(&old) {
                    self.total -= b.byte_len();
                }
            }
        }
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(b) = self.entries.remove(id) {
            self.total -= b.byte_len();
            self.order.retain(|k| k != id);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.total
    }

    fn touch(&mut self, id: &str) {
        self.order.retain(|k| k != id);
        self.order.push_back(id.to_string());
    }
}
```

- [ ] **Step 5: Implement fake_backend.rs**

Top of `core/src/audio/fake_backend.rs`:
```rust
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Play { handle: SinkHandle, target: DeviceTarget, gain: f32, samples: usize },
    Stop(SinkHandle),
    SetGain(SinkHandle, f32),
    Reset,
}

/// Records every call; the first device name given is the default device.
pub struct FakeBackend {
    devices: Vec<DeviceInfo>,
    next: AtomicU64,
    calls: Mutex<Vec<Call>>,
    finished: Mutex<HashSet<SinkHandle>>,
    gains: Mutex<HashMap<SinkHandle, f32>>,
}

impl FakeBackend {
    pub fn new(device_names: &[&str]) -> Self {
        let devices = device_names
            .iter()
            .enumerate()
            .map(|(i, n)| DeviceInfo { name: n.to_string(), is_default: i == 0 })
            .collect();
        Self {
            devices,
            next: AtomicU64::new(1),
            calls: Mutex::new(Vec::new()),
            finished: Mutex::new(HashSet::new()),
            gains: Mutex::new(HashMap::new()),
        }
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// Handles in play order.
    pub fn handles(&self) -> Vec<SinkHandle> {
        self.calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Play { handle, .. } => Some(handle),
                _ => None,
            })
            .collect()
    }

    pub fn mark_finished(&self, handle: SinkHandle) {
        self.finished.lock().unwrap().insert(handle);
    }

    pub fn gain(&self, handle: SinkHandle) -> Option<f32> {
        self.gains.lock().unwrap().get(&handle).copied()
    }
}

impl AudioBackend for FakeBackend {
    fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.clone()
    }

    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        if let DeviceTarget::Named(n) = target {
            if !self.devices.iter().any(|d| &d.name == n) {
                return Err(AudioError::DeviceNotFound(n.clone()));
            }
        }
        let handle = SinkHandle(self.next.fetch_add(1, Ordering::SeqCst));
        self.calls.lock().unwrap().push(Call::Play {
            handle,
            target: target.clone(),
            gain,
            samples: buffer.samples.len(),
        });
        self.gains.lock().unwrap().insert(handle, gain);
        Ok(handle)
    }

    fn stop(&self, handle: SinkHandle) {
        self.calls.lock().unwrap().push(Call::Stop(handle));
        self.finished.lock().unwrap().insert(handle);
    }

    fn set_gain(&self, handle: SinkHandle, gain: f32) {
        self.calls.lock().unwrap().push(Call::SetGain(handle, gain));
        self.gains.lock().unwrap().insert(handle, gain);
    }

    fn is_finished(&self, handle: SinkHandle) -> bool {
        self.finished.lock().unwrap().contains(&handle)
    }

    fn reset(&self) {
        self.calls.lock().unwrap().push(Call::Reset);
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p soundboard-core audio`
Expected: 7 passed.

- [ ] **Step 7: Commit**

```bash
git add core/src/audio core/src/lib.rs
git commit -m "feat(core): audio backend trait, LRU buffer cache, fake backend"
```

---

### Task 4: Decoder and shared wav test helper

**Files:**
- Create: `core/src/audio/decode.rs`, `core/src/test_util.rs`
- Modify: `core/src/audio/mod.rs`, `core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub const SUPPORTED_EXTENSIONS: [&str; 4]`
  - `pub fn has_supported_extension(path: &Path) -> bool`
  - `pub fn decode_file(path: &Path) -> Result<DecodedBuffer, AudioError>`
  - test-only `soundboard_core::test_util::write_test_wav(path: &Path, frames: u32)` writing 16-bit stereo 44.1 kHz

- [ ] **Step 1: Write the helper and failing tests**

`core/src/test_util.rs`:
```rust
//! Helpers shared by unit tests. Compiled only under `cfg(test)`.

use std::path::Path;

/// Writes a 16-bit stereo 44.1 kHz WAV with `frames` frames of a quiet sawtooth.
pub fn write_test_wav(path: &Path, frames: u32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..frames {
        let v = ((i % 100) as i16 - 50) * 100;
        w.write_sample(v).unwrap();
        w.write_sample(v).unwrap();
    }
    w.finalize().unwrap();
}
```

Add to `core/src/lib.rs`:
```rust
#[cfg(test)]
pub mod test_util;
```

Add to `core/src/audio/mod.rs`:
```rust
pub mod decode;
```

Tests at bottom of `core/src/audio/decode.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::write_test_wav;
    use tempfile::tempdir;

    #[test]
    fn decodes_wav_to_expected_sample_count() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 1000);
        let b = decode_file(&p).unwrap();
        assert_eq!(b.channels, 2);
        assert_eq!(b.sample_rate, 44100);
        assert_eq!(b.samples.len(), 2000);
        assert!((b.duration_secs() - 1000.0 / 44100.0).abs() < 1e-6);
    }

    #[test]
    fn garbage_file_is_decode_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("bad.wav");
        std::fs::write(&p, b"this is not audio at all").unwrap();
        assert!(matches!(decode_file(&p), Err(AudioError::Decode(_))));
    }

    #[test]
    fn missing_file_is_io_error() {
        assert!(matches!(decode_file(Path::new("Z:/nope/missing.wav")), Err(AudioError::Io(_))));
    }

    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(has_supported_extension(Path::new("a.MP3")));
        assert!(has_supported_extension(Path::new("a.flac")));
        assert!(!has_supported_extension(Path::new("a.txt")));
        assert!(!has_supported_extension(Path::new("noext")));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core decode`
Expected: compile error, `decode_file` not found.

- [ ] **Step 3: Implement**

Top of `core/src/audio/decode.rs`:
```rust
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rodio::{Decoder, Source};

use super::backend::{AudioError, DecodedBuffer};

pub const SUPPORTED_EXTENSIONS: [&str; 4] = ["mp3", "wav", "ogg", "flac"];

pub fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Fully decodes an audio file into interleaved f32 samples.
pub fn decode_file(path: &Path) -> Result<DecodedBuffer, AudioError> {
    let file = File::open(path)?;
    let decoder = Decoder::new(BufReader::new(file))
        .map_err(|e| AudioError::Decode(format!("{}: {e}", path.display())))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();
    if samples.is_empty() {
        return Err(AudioError::Decode(format!("{}: no audio data", path.display())));
    }
    Ok(DecodedBuffer::new(channels, sample_rate, samples))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p soundboard-core decode`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add core/src
git commit -m "feat(core): file decoder and wav test helper"
```

---

### Task 5: Audio engine

**Files:**
- Create: `core/src/audio/engine.rs`
- Modify: `core/src/audio/mod.rs`

**Interfaces:**
- Consumes: `AudioBackend`, `BufferCache`, `decode_file`, `DeviceTarget`, `SinkHandle`, `DecodedBuffer`.
- Produces:
  - `pub struct Outputs { speakers: Option<String>, virtual_mic: Option<String> }` with `targets() -> Vec<DeviceTarget>` (deduplicated, Named only)
  - `pub struct PlayOutcome { instance: u64, sinks: usize, fallbacks: Vec<String> }`
  - `pub struct Engine<B: AudioBackend>` with `new(backend, outputs, master, cache_bytes)`, `backend()`, `devices()`, `outputs()`, `set_outputs(Outputs)`, `master()`, `set_master(f32)`, `set_sound_volume(&str, f32)`, `play_file(sound_id, sound_volume, &Path) -> Result<PlayOutcome, AudioError>`, `play_buffer(sound_id, sound_volume, &DecodedBuffer) -> Result<PlayOutcome, AudioError>`, `stop_all()`, `reap() -> bool`, `playing() -> Vec<String>`, `invalidate(&str)`, `cached() -> usize`, `test_tone(&DeviceTarget) -> Result<SinkHandle, AudioError>`
  - `pub fn sine(freq: f32, secs: f32, sample_rate: u32) -> DecodedBuffer`
  - `pub const TEST_TONE_ID: &str = "__test_tone__"` (excluded from `playing()`)

- [ ] **Step 1: Write failing tests**

Bottom of `core/src/audio/engine.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fake_backend::{Call, FakeBackend};
    use crate::test_util::write_test_wav;
    use tempfile::tempdir;

    fn both() -> Outputs {
        Outputs { speakers: Some("Speakers".into()), virtual_mic: Some("CABLE Input".into()) }
    }

    fn engine(outputs: Outputs, master: f32) -> Engine<FakeBackend> {
        Engine::new(FakeBackend::new(&["Speakers", "CABLE Input"]), outputs, master, 1024 * 1024)
    }

    fn clip() -> DecodedBuffer {
        DecodedBuffer::new(1, 44100, vec![0.1; 100])
    }

    #[test]
    fn play_fans_out_to_each_output_with_combined_gain() {
        let mut e = engine(both(), 0.5);
        let out = e.play_buffer("a", 0.8, &clip()).unwrap();
        assert_eq!(out.sinks, 2);
        assert!(out.fallbacks.is_empty());
        let plays: Vec<_> = e.backend().calls().into_iter().filter(|c| matches!(c, Call::Play { .. })).collect();
        assert_eq!(plays.len(), 2);
        for p in plays {
            if let Call::Play { target, gain, samples, .. } = p {
                assert!(matches!(target, DeviceTarget::Named(_)));
                assert!((gain - 0.4).abs() < 1e-6);
                assert_eq!(samples, 100);
            }
        }
        assert_eq!(e.playing(), vec!["a".to_string()]);
    }

    #[test]
    fn missing_named_device_falls_back_to_default_and_reports() {
        let mut e = engine(Outputs { speakers: Some("Ghost".into()), virtual_mic: None }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(out.sinks, 1);
        assert_eq!(out.fallbacks, vec!["Ghost".to_string()]);
        assert!(matches!(e.backend().calls()[0], Call::Play { target: DeviceTarget::Default, .. }));
    }

    #[test]
    fn no_outputs_is_error() {
        let mut e = engine(Outputs::default(), 1.0);
        assert!(matches!(e.play_buffer("a", 1.0, &clip()), Err(AudioError::NoOutputs)));
    }

    #[test]
    fn same_device_in_both_slots_plays_once() {
        let mut e = engine(Outputs { speakers: Some("Speakers".into()), virtual_mic: Some("Speakers".into()) }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(out.sinks, 1);
    }

    #[test]
    fn stop_all_stops_every_handle_and_clears_playing() {
        let mut e = engine(both(), 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        e.play_buffer("b", 1.0, &clip()).unwrap();
        e.stop_all();
        assert!(e.playing().is_empty());
        let stops = e.backend().calls().into_iter().filter(|c| matches!(c, Call::Stop(_))).count();
        assert_eq!(stops, 4);
    }

    #[test]
    fn set_master_updates_active_gains() {
        let mut e = engine(both(), 0.5);
        e.play_buffer("a", 0.8, &clip()).unwrap();
        e.set_master(1.0);
        for h in e.backend().handles() {
            assert!((e.backend().gain(h).unwrap() - 0.8).abs() < 1e-6);
        }
    }

    #[test]
    fn set_sound_volume_only_touches_matching_sound() {
        let mut e = engine(Outputs { speakers: Some("Speakers".into()), virtual_mic: None }, 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        e.play_buffer("b", 1.0, &clip()).unwrap();
        e.set_sound_volume("a", 0.2);
        let hs = e.backend().handles();
        assert!((e.backend().gain(hs[0]).unwrap() - 0.2).abs() < 1e-6);
        assert!((e.backend().gain(hs[1]).unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn reap_removes_finished_and_reports_change() {
        let mut e = engine(both(), 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        e.play_buffer("b", 1.0, &clip()).unwrap();
        let hs = e.backend().handles();
        e.backend().mark_finished(hs[0]);
        e.backend().mark_finished(hs[1]);
        assert!(e.reap());
        assert_eq!(e.playing(), vec!["b".to_string()]);
        assert!(!e.reap(), "nothing changed on second reap");
    }

    #[test]
    fn play_file_decodes_once_then_uses_cache() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 500);
        let mut e = engine(both(), 1.0);
        e.play_file("a", 1.0, &p).unwrap();
        e.play_file("a", 1.0, &p).unwrap();
        assert_eq!(e.cached(), 1);
        e.invalidate("a");
        assert_eq!(e.cached(), 0);
    }

    #[test]
    fn play_file_missing_is_io_error() {
        let mut e = engine(both(), 1.0);
        assert!(matches!(e.play_file("a", 1.0, Path::new("Z:/nope.wav")), Err(AudioError::Io(_))));
    }

    #[test]
    fn set_outputs_resets_backend_and_stops_sound() {
        let mut e = engine(both(), 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        e.set_outputs(Outputs { speakers: Some("Speakers".into()), virtual_mic: None });
        assert!(e.playing().is_empty());
        assert!(e.backend().calls().contains(&Call::Reset));
        assert_eq!(e.outputs().targets().len(), 1);
    }

    #[test]
    fn test_tone_not_listed_in_playing() {
        let mut e = engine(both(), 1.0);
        e.test_tone(&DeviceTarget::Named("Speakers".into())).unwrap();
        assert!(e.playing().is_empty());
        assert_eq!(e.backend().handles().len(), 1);
    }

    #[test]
    fn sine_has_expected_length() {
        let s = sine(440.0, 0.4, 44100);
        assert_eq!(s.samples.len(), 17640);
        assert_eq!(s.channels, 1);
    }
}
```

Add to `core/src/audio/mod.rs`:
```rust
pub mod engine;
pub use engine::{sine, Engine, Outputs, PlayOutcome, TEST_TONE_ID};
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core engine`
Expected: compile error, `Engine` not found.

- [ ] **Step 3: Implement**

Top of `core/src/audio/engine.rs`:
```rust
use std::collections::HashSet;
use std::path::Path;

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};
use super::cache::BufferCache;
use super::decode::decode_file;

pub const TEST_TONE_ID: &str = "__test_tone__";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outputs {
    pub speakers: Option<String>,
    pub virtual_mic: Option<String>,
}

impl Outputs {
    /// Named targets for every configured slot, deduplicated, in slot order.
    pub fn targets(&self) -> Vec<DeviceTarget> {
        let mut out: Vec<DeviceTarget> = Vec::new();
        for name in [&self.speakers, &self.virtual_mic].into_iter().flatten() {
            let t = DeviceTarget::Named(name.clone());
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayOutcome {
    pub instance: u64,
    pub sinks: usize,
    /// Device names that were missing; sound went to the default device instead.
    pub fallbacks: Vec<String>,
}

struct Active {
    handle: SinkHandle,
    sound_id: String,
    sound_volume: f32,
}

pub struct Engine<B: AudioBackend> {
    backend: B,
    cache: BufferCache,
    master: f32,
    outputs: Outputs,
    active: Vec<Active>,
    next_instance: u64,
}

impl<B: AudioBackend> Engine<B> {
    pub fn new(backend: B, outputs: Outputs, master: f32, cache_bytes: usize) -> Self {
        Self {
            backend,
            cache: BufferCache::new(cache_bytes),
            master: master.clamp(0.0, 1.0),
            outputs,
            active: Vec::new(),
            next_instance: 1,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.backend.devices()
    }

    pub fn outputs(&self) -> &Outputs {
        &self.outputs
    }

    /// Stops everything, drops device streams, and switches outputs.
    pub fn set_outputs(&mut self, outputs: Outputs) {
        self.stop_all();
        self.backend.reset();
        self.outputs = outputs;
    }

    pub fn master(&self) -> f32 {
        self.master
    }

    pub fn set_master(&mut self, v: f32) {
        self.master = v.clamp(0.0, 1.0);
        for a in &self.active {
            self.backend.set_gain(a.handle, a.sound_volume * self.master);
        }
    }

    pub fn set_sound_volume(&mut self, sound_id: &str, v: f32) {
        let v = v.clamp(0.0, 1.0);
        let master = self.master;
        let backend = &self.backend;
        for a in self.active.iter_mut().filter(|a| a.sound_id == sound_id) {
            a.sound_volume = v;
            backend.set_gain(a.handle, v * master);
        }
    }

    pub fn play_file(&mut self, sound_id: &str, sound_volume: f32, path: &Path) -> Result<PlayOutcome, AudioError> {
        let buffer = match self.cache.get(sound_id) {
            Some(b) => b,
            None => {
                let b = decode_file(path)?;
                self.cache.insert(sound_id, b.clone());
                b
            }
        };
        self.play_buffer(sound_id, sound_volume, &buffer)
    }

    pub fn play_buffer(&mut self, sound_id: &str, sound_volume: f32, buffer: &DecodedBuffer) -> Result<PlayOutcome, AudioError> {
        let targets = self.outputs.targets();
        if targets.is_empty() {
            return Err(AudioError::NoOutputs);
        }
        let sound_volume = sound_volume.clamp(0.0, 1.0);
        let gain = sound_volume * self.master;
        let instance = self.next_instance;
        self.next_instance += 1;

        let mut fallbacks = Vec::new();
        let mut sinks = 0;
        let mut last_err = None;
        for target in targets {
            let result = match self.backend.play(&target, buffer, gain) {
                Err(AudioError::DeviceNotFound(name)) => {
                    fallbacks.push(name);
                    self.backend.play(&DeviceTarget::Default, buffer, gain)
                }
                other => other,
            };
            match result {
                Ok(handle) => {
                    self.active.push(Active { handle, sound_id: sound_id.to_string(), sound_volume });
                    sinks += 1;
                }
                Err(e) => last_err = Some(e),
            }
        }
        if sinks == 0 {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(PlayOutcome { instance, sinks, fallbacks })
    }

    pub fn stop_all(&mut self) {
        for a in self.active.drain(..) {
            self.backend.stop(a.handle);
        }
    }

    /// Drops finished sinks. Returns true when the set of playing sounds changed.
    pub fn reap(&mut self) -> bool {
        let before = self.playing();
        let backend = &self.backend;
        self.active.retain(|a| !backend.is_finished(a.handle));
        before != self.playing()
    }

    /// Unique sound ids with at least one active sink, in start order. Test tones excluded.
    pub fn playing(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.active
            .iter()
            .filter(|a| a.sound_id != TEST_TONE_ID)
            .filter(|a| seen.insert(a.sound_id.clone()))
            .map(|a| a.sound_id.clone())
            .collect()
    }

    pub fn invalidate(&mut self, sound_id: &str) {
        self.cache.remove(sound_id);
    }

    pub fn cached(&self) -> usize {
        self.cache.len()
    }

    /// 0.4 s 440 Hz tone at 30 % of master, on one explicit target.
    pub fn test_tone(&mut self, target: &DeviceTarget) -> Result<SinkHandle, AudioError> {
        let buffer = sine(440.0, 0.4, 44100);
        let handle = self.backend.play(target, &buffer, 0.3 * self.master)?;
        self.active.push(Active { handle, sound_id: TEST_TONE_ID.to_string(), sound_volume: 0.3 });
        Ok(handle)
    }
}

/// Mono sine with a linear fade-out so it does not click.
pub fn sine(freq: f32, secs: f32, sample_rate: u32) -> DecodedBuffer {
    let n = (secs * sample_rate as f32) as usize;
    let samples = (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let env = 1.0 - i as f32 / n as f32;
            (t * freq * std::f32::consts::TAU).sin() * env
        })
        .collect();
    DecodedBuffer::new(1, sample_rate, samples)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p soundboard-core audio`
Expected: all audio tests pass (cache 5, fake 2, decode 4, engine 13).

- [ ] **Step 5: Commit**

```bash
git add core/src/audio
git commit -m "feat(core): audio engine with fan-out, gains, fallback and reaping"
```

---
### Task 6: Rodio backend on a dedicated audio thread

**Files:**
- Create: `core/src/audio/rodio_backend.rs`, `core/examples/tone.rs`
- Modify: `core/src/audio/mod.rs`

**Interfaces:**
- Consumes: `AudioBackend` trait and value types from Task 3.
- Produces: `pub struct RodioBackend` with `new() -> Self` and `Default`, implementing `AudioBackend`. Safe to put inside `Arc<Mutex<..>>` (it only holds an `mpsc::Sender`).

Why a thread: cpal `Stream` is not thread safe, so every `OutputStream` lives on one thread that owns them and receives commands over a channel.

- [ ] **Step 1: Write the (mostly manual) tests**

Bottom of `core/src/audio/rodio_backend.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_does_not_panic_without_hardware() {
        let b = RodioBackend::new();
        let _ = b.devices();
        b.reset();
    }

    /// Needs a real output device. Run with: cargo test -p soundboard-core plays_tone -- --ignored --nocapture
    #[test]
    #[ignore]
    fn plays_tone_on_default_device() {
        let b = RodioBackend::new();
        let tone = crate::audio::engine::sine(440.0, 0.3, 44100);
        let h = b.play(&DeviceTarget::Default, &tone, 0.3).unwrap();
        assert!(!b.is_finished(h));
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(b.is_finished(h));
    }

    #[test]
    fn unknown_named_device_is_not_found() {
        let b = RodioBackend::new();
        let tone = crate::audio::engine::sine(440.0, 0.01, 44100);
        match b.play(&DeviceTarget::Named("definitely-not-a-device".into()), &tone, 0.1) {
            Err(AudioError::DeviceNotFound(_)) => {}
            Err(AudioError::Stream(_)) => {} // machines with no audio host at all
            other => panic!("unexpected {other:?}"),
        }
    }
}
```

Add to `core/src/audio/mod.rs`:
```rust
pub mod rodio_backend;
pub use rodio_backend::RodioBackend;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core rodio`
Expected: compile error, `RodioBackend` not found.

- [ ] **Step 3: Implement**

Top of `core/src/audio/rodio_backend.rs`:
```rust
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use rodio::buffer::SamplesBuffer;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle, Sink};

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};

enum Cmd {
    Devices(Sender<Vec<DeviceInfo>>),
    Play { target: DeviceTarget, buffer: DecodedBuffer, gain: f32, reply: Sender<Result<SinkHandle, AudioError>> },
    Stop(SinkHandle),
    SetGain(SinkHandle, f32),
    IsFinished(SinkHandle, Sender<bool>),
    Reset,
}

/// Real backend. All cpal/rodio objects live on the `audio` thread.
pub struct RodioBackend {
    tx: Sender<Cmd>,
}

impl RodioBackend {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || audio_thread(rx))
            .expect("spawn audio thread");
        Self { tx }
    }

    fn gone() -> AudioError {
        AudioError::Stream("audio thread is gone".into())
    }
}

impl Default for RodioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBackend for RodioBackend {
    fn devices(&self) -> Vec<DeviceInfo> {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::Devices(tx)).is_err() {
            return Vec::new();
        }
        rx.recv().unwrap_or_default()
    }

    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Cmd::Play { target: target.clone(), buffer: buffer.clone(), gain, reply: tx })
            .map_err(|_| Self::gone())?;
        rx.recv().map_err(|_| Self::gone())?
    }

    fn stop(&self, handle: SinkHandle) {
        let _ = self.tx.send(Cmd::Stop(handle));
    }

    fn set_gain(&self, handle: SinkHandle, gain: f32) {
        let _ = self.tx.send(Cmd::SetGain(handle, gain));
    }

    fn is_finished(&self, handle: SinkHandle) -> bool {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::IsFinished(handle, tx)).is_err() {
            return true;
        }
        rx.recv().unwrap_or(true)
    }

    fn reset(&self) {
        let _ = self.tx.send(Cmd::Reset);
    }
}

struct AudioThread {
    host: rodio::cpal::Host,
    streams: HashMap<DeviceTarget, (OutputStream, OutputStreamHandle)>,
    sinks: HashMap<SinkHandle, Sink>,
    next: u64,
}

fn audio_thread(rx: Receiver<Cmd>) {
    let mut st = AudioThread {
        host: rodio::cpal::default_host(),
        streams: HashMap::new(),
        sinks: HashMap::new(),
        next: 1,
    };
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Devices(reply) => {
                let _ = reply.send(st.devices());
            }
            Cmd::Play { target, buffer, gain, reply } => {
                let _ = reply.send(st.play(&target, &buffer, gain));
            }
            Cmd::Stop(h) => {
                if let Some(s) = st.sinks.remove(&h) {
                    s.stop();
                }
            }
            Cmd::SetGain(h, g) => {
                if let Some(s) = st.sinks.get(&h) {
                    s.set_volume(g);
                }
            }
            Cmd::IsFinished(h, reply) => {
                let done = st.sinks.get(&h).map(|s| s.empty()).unwrap_or(true);
                if done {
                    st.sinks.remove(&h);
                }
                let _ = reply.send(done);
            }
            Cmd::Reset => {
                st.sinks.clear();
                st.streams.clear();
            }
        }
    }
}

impl AudioThread {
    fn devices(&self) -> Vec<DeviceInfo> {
        let default = self.host.default_output_device().and_then(|d| d.name().ok());
        match self.host.output_devices() {
            Ok(iter) => iter
                .filter_map(|d| d.name().ok())
                .map(|name| DeviceInfo { is_default: Some(&name) == default.as_ref(), name })
                .collect(),
            Err(e) => {
                log::warn!("enumerating output devices failed: {e}");
                Vec::new()
            }
        }
    }

    fn find_device(&self, target: &DeviceTarget) -> Result<rodio::cpal::Device, AudioError> {
        match target {
            DeviceTarget::Default => self
                .host
                .default_output_device()
                .ok_or_else(|| AudioError::Stream("no default output device".into())),
            DeviceTarget::Named(name) => self
                .host
                .output_devices()
                .map_err(|e| AudioError::Stream(e.to_string()))?
                .find(|d| d.name().map(|n| &n == name).unwrap_or(false))
                .ok_or_else(|| AudioError::DeviceNotFound(name.clone())),
        }
    }

    fn handle_for(&mut self, target: &DeviceTarget) -> Result<OutputStreamHandle, AudioError> {
        if !self.streams.contains_key(target) {
            let device = self.find_device(target)?;
            let (stream, handle) =
                OutputStream::try_from_device(&device).map_err(|e| AudioError::Stream(e.to_string()))?;
            self.streams.insert(target.clone(), (stream, handle));
        }
        Ok(self.streams[target].1.clone())
    }

    fn play(&mut self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        let sink = match self.handle_for(target).and_then(|h| Sink::try_new(&h).map_err(|e| AudioError::Stream(e.to_string()))) {
            Ok(s) => s,
            Err(e) => {
                // The stream may have died (device unplugged). Drop it so the next attempt reopens,
                // and report DeviceNotFound if the device is really gone so the engine can fall back.
                self.streams.remove(target);
                return match self.find_device(target) {
                    Err(not_found @ AudioError::DeviceNotFound(_)) => Err(not_found),
                    _ => Err(e),
                };
            }
        };
        sink.set_volume(gain);
        sink.append(SamplesBuffer::new(buffer.channels, buffer.sample_rate, (*buffer.samples).clone()));
        let h = SinkHandle(self.next);
        self.next += 1;
        self.sinks.insert(h, sink);
        Ok(h)
    }
}
```

- [ ] **Step 4: Manual check example**

`core/examples/tone.rs`:
```rust
//! Lists output devices and plays a test tone.
//! cargo run -p soundboard-core --example tone            -> default device
//! cargo run -p soundboard-core --example tone "CABLE Input (VB-Audio Virtual Cable)"
use soundboard_core::audio::{sine, AudioBackend, DeviceTarget, RodioBackend};

fn main() {
    let backend = RodioBackend::new();
    for d in backend.devices() {
        println!("{}{}", d.name, if d.is_default { "  (default)" } else { "" });
    }
    let target = DeviceTarget::from_name(std::env::args().nth(1).as_deref());
    println!("playing on {target:?}");
    let h = backend.play(&target, &sine(440.0, 0.6, 44100), 0.3).expect("play");
    while !backend.is_finished(h) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
```

- [ ] **Step 5: Run tests and the example**

Run: `cargo test -p soundboard-core rodio`
Expected: 2 passed, 1 ignored.

Run: `cargo test -p soundboard-core plays_tone -- --ignored`
Expected: a 0.3 s beep is heard, test passes.

Run: `cargo run -p soundboard-core --example tone`
Expected: device list printed, beep heard on the default device.

- [ ] **Step 6: Commit**

```bash
git add core/src/audio core/examples
git commit -m "feat(core): rodio backend on a dedicated audio thread"
```

---

### Task 7: Library (sound and image files)

**Files:**
- Create: `core/src/library.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: `config::Sound`, `audio::decode::{decode_file, has_supported_extension}`.
- Produces:
  - `pub struct Library` with `open(root) -> io::Result<Self>`, `root()`, `config_path()`, `sounds_dir()`, `images_dir()`, `sound_path(&Sound) -> PathBuf`, `image_path(&Sound) -> Option<PathBuf>`, `import_sound(&Path) -> Result<Sound, LibraryError>`, `import_image(&mut Sound, &Path) -> Result<(), LibraryError>`, `remove_image(&mut Sound) -> io::Result<()>`, `delete_sound(&Sound) -> io::Result<()>`
  - `pub enum LibraryError { UnsupportedExtension(String), Undecodable(String), NotAnImage(String), Io(io::Error) }` (Display messages are user-facing sentences)
  - `pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf`
  - `pub const IMAGE_MIMES: [&str; 4]`

- [ ] **Step 1: Write failing tests**

Bottom of `core/src/library.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::write_test_wav;
    use tempfile::tempdir;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00";

    fn setup() -> (tempfile::TempDir, Library, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let lib = Library::open(dir.path().join("data")).unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        (dir, lib, src)
    }

    #[test]
    fn open_creates_folders() {
        let (_d, lib, _) = setup();
        assert!(lib.sounds_dir().is_dir());
        assert!(lib.images_dir().is_dir());
        assert_eq!(lib.config_path().file_name().unwrap(), "config.json");
    }

    #[test]
    fn import_rejects_unsupported_extension() {
        let (_d, lib, src) = setup();
        let p = src.join("notes.txt");
        std::fs::write(&p, "hello").unwrap();
        assert!(matches!(lib.import_sound(&p), Err(LibraryError::UnsupportedExtension(n)) if n == "notes.txt"));
    }

    #[test]
    fn import_rejects_undecodable() {
        let (_d, lib, src) = setup();
        let p = src.join("fake.wav");
        std::fs::write(&p, "not a wav").unwrap();
        assert!(matches!(lib.import_sound(&p), Err(LibraryError::Undecodable(n)) if n == "fake.wav"));
        assert_eq!(std::fs::read_dir(lib.sounds_dir()).unwrap().count(), 0, "nothing copied");
    }

    #[test]
    fn import_copies_and_renames_on_collision() {
        let (_d, lib, src) = setup();
        let p = src.join("clip.wav");
        write_test_wav(&p, 100);
        let a = lib.import_sound(&p).unwrap();
        let b = lib.import_sound(&p).unwrap();
        assert_eq!(a.name, "clip");
        assert_eq!(b.name, "clip");
        assert_eq!(a.file, "clip.wav");
        assert_eq!(b.file, "clip (2).wav");
        assert_ne!(a.id, b.id);
        assert_eq!(a.volume, 1.0);
        assert!(lib.sound_path(&a).exists());
        assert!(lib.sound_path(&b).exists());
    }

    #[test]
    fn import_image_validates_magic_and_replaces_old() {
        let (_d, lib, src) = setup();
        let wav = src.join("clip.wav");
        write_test_wav(&wav, 10);
        let mut s = lib.import_sound(&wav).unwrap();

        let txt = src.join("x.png");
        std::fs::write(&txt, "not an image even with png extension").unwrap();
        assert!(matches!(lib.import_image(&mut s, &txt), Err(LibraryError::NotAnImage(_))));
        assert_eq!(s.image, None);

        let png = src.join("pic.bin");
        std::fs::write(&png, PNG).unwrap();
        lib.import_image(&mut s, &png).unwrap();
        assert_eq!(s.image.as_deref(), Some(format!("{}.png", s.id).as_str()));
        let png_path = lib.image_path(&s).unwrap();
        assert!(png_path.exists());

        let gif = src.join("anim.gif");
        std::fs::write(&gif, GIF).unwrap();
        lib.import_image(&mut s, &gif).unwrap();
        assert_eq!(s.image.as_deref(), Some(format!("{}.gif", s.id).as_str()));
        assert!(!png_path.exists(), "old image removed");

        lib.remove_image(&mut s).unwrap();
        assert_eq!(s.image, None);
        assert_eq!(std::fs::read_dir(lib.images_dir()).unwrap().count(), 0);
    }

    #[test]
    fn delete_removes_sound_and_image() {
        let (_d, lib, src) = setup();
        let wav = src.join("clip.wav");
        write_test_wav(&wav, 10);
        let mut s = lib.import_sound(&wav).unwrap();
        let png = src.join("p.png");
        std::fs::write(&png, PNG).unwrap();
        lib.import_image(&mut s, &png).unwrap();
        lib.delete_sound(&s).unwrap();
        assert!(!lib.sound_path(&s).exists());
        assert!(!lib.image_path(&s).unwrap().exists());
        lib.delete_sound(&s).unwrap(); // idempotent
    }

    #[test]
    fn unique_path_counts_up() {
        let dir = tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "a.mp3"), dir.path().join("a.mp3"));
        std::fs::write(dir.path().join("a.mp3"), "").unwrap();
        std::fs::write(dir.path().join("a (2).mp3"), "").unwrap();
        assert_eq!(unique_path(dir.path(), "a.mp3"), dir.path().join("a (3).mp3"));
    }
}
```

Add to `core/src/lib.rs`:
```rust
pub mod library;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core library`
Expected: compile error, `Library` not found.

- [ ] **Step 3: Implement**

Top of `core/src/library.rs`:
```rust
//! Files on disk: `<root>/sounds/*` and `<root>/images/<sound id>.<ext>`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::audio::decode::{decode_file, has_supported_extension};
use crate::config::Sound;

pub const IMAGE_MIMES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("{0} is not an mp3, wav, ogg or flac file")]
    UnsupportedExtension(String),
    #[error("{0} could not be decoded")]
    Undecodable(String),
    #[error("{0} is not a png, jpg, webp or gif image")]
    NotAnImage(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Library {
    root: PathBuf,
}

fn display_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string()
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("sounds"))?;
        fs::create_dir_all(root.join("images"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn sounds_dir(&self) -> PathBuf {
        self.root.join("sounds")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.root.join("images")
    }

    pub fn sound_path(&self, sound: &Sound) -> PathBuf {
        self.sounds_dir().join(&sound.file)
    }

    pub fn image_path(&self, sound: &Sound) -> Option<PathBuf> {
        sound.image.as_ref().map(|i| self.images_dir().join(i))
    }

    /// Validates, copies into `sounds/` (renaming on collision) and returns a new `Sound`.
    pub fn import_sound(&self, source: &Path) -> Result<Sound, LibraryError> {
        let display = display_name(source);
        if !has_supported_extension(source) {
            return Err(LibraryError::UnsupportedExtension(display));
        }
        decode_file(source).map_err(|_| LibraryError::Undecodable(display.clone()))?;
        let dest = unique_path(&self.sounds_dir(), &display);
        fs::copy(source, &dest)?;
        let name = source.file_stem().and_then(|s| s.to_str()).unwrap_or("Sound").to_string();
        Ok(Sound {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            file: display_name(&dest),
            image: None,
            volume: 1.0,
            hotkey: None,
        })
    }

    /// Validates by magic bytes, copies to `images/<id>.<ext>`, removes any previous image.
    pub fn import_image(&self, sound: &mut Sound, source: &Path) -> Result<(), LibraryError> {
        let display = display_name(source);
        let kind = infer::get_from_path(source)?
            .filter(|k| IMAGE_MIMES.contains(&k.mime_type()))
            .ok_or(LibraryError::NotAnImage(display))?;
        let file_name = format!("{}.{}", sound.id, kind.extension());
        let dest = self.images_dir().join(&file_name);
        self.remove_image(sound)?;
        fs::copy(source, &dest)?;
        sound.image = Some(file_name);
        Ok(())
    }

    pub fn remove_image(&self, sound: &mut Sound) -> io::Result<()> {
        if let Some(p) = self.image_path(sound) {
            remove_if_exists(&p)?;
        }
        sound.image = None;
        Ok(())
    }

    pub fn delete_sound(&self, sound: &Sound) -> io::Result<()> {
        remove_if_exists(&self.sound_path(sound))?;
        if let Some(p) = self.image_path(sound) {
            remove_if_exists(&p)?;
        }
        Ok(())
    }
}

/// `dir/file.ext`, or `dir/file (2).ext`, `dir/file (3).ext`, ... whichever is free.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(file_name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = p.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    (2u32..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .expect("some suffix is free")
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p soundboard-core library`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add core/src/library.rs core/src/lib.rs
git commit -m "feat(core): library imports sounds and images with validation"
```

---

### Task 8: Hotkey rules

**Files:**
- Create: `core/src/hotkeys.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: `config::Config`.
- Produces:
  - `pub enum Owner<'a> { Sound(&'a str), StopAll }`
  - `pub enum HotkeyError { Invalid(String), Conflict { accelerator: String, owner: String } }` (Display is user-facing)
  - `pub fn normalize(input: &str) -> Result<String, HotkeyError>`
  - `pub fn requires_modifier(key_code: &str) -> bool` (false only for `F1`..`F24`)
  - `pub fn find_conflict(cfg: &Config, accelerator: &str, exclude: Owner) -> Option<String>` returns owner name (`sound.name` or `"Stop All"`)
  - `pub fn validate_binding(cfg: &Config, input: &str, exclude: Owner) -> Result<String, HotkeyError>` = normalize + modifier rule + conflict
  - `pub const STOP_ALL_OWNER: &str = "Stop All"`

- [ ] **Step 1: Write failing tests**

Bottom of `core/src/hotkeys.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Sound};

    fn cfg() -> Config {
        let mut c = Config::default();
        c.stop_all_hotkey = "Ctrl+Shift+F12".into();
        c.sounds.push(Sound { id: "a".into(), name: "Airhorn".into(), file: "a.wav".into(), image: None, volume: 1.0, hotkey: Some("Ctrl+Shift+Digit1".into()) });
        c.sounds.push(Sound { id: "b".into(), name: "Bruh".into(), file: "b.wav".into(), image: None, volume: 1.0, hotkey: None });
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
```

Add to `core/src/lib.rs`:
```rust
pub mod hotkeys;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core hotkeys`
Expected: compile error.

- [ ] **Step 3: Implement**

Top of `core/src/hotkeys.rs`:
```rust
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p soundboard-core hotkeys`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add core/src/hotkeys.rs core/src/lib.rs
git commit -m "feat(core): hotkey normalization and conflict rules"
```

---

### Task 9: Remote auth (PIN, tokens, lockout) and protocol types

**Files:**
- Create: `core/src/remote/mod.rs` (module declarations only for now), `core/src/remote/auth.rs`, `core/src/remote/protocol.rs`
- Modify: `core/src/lib.rs`

**Interfaces:**
- Produces (`remote::auth`):
  - `pub fn valid_pin_format(pin: &str) -> bool`
  - `pub fn hash_pin(pin: &str) -> String`, `pub fn verify_pin(pin: &str, hash: &str) -> bool`
  - `pub fn issue_token(secret: &str) -> String`, `pub fn verify_token(secret: &str, token: &str) -> bool`
  - `pub struct Lockout` with `new(max_failures: u32, window: Duration, block: Duration)`, `standard()` (5, 600 s, 60 s), `is_blocked(&mut self, ip: IpAddr, now: Instant) -> bool`, `record_failure(&mut self, ip, now)`, `record_success(&mut self, ip)`
- Produces (`remote::protocol`):
  - `pub struct RemoteSound { id: String, name: String, volume: f32, has_image: bool }`
  - `pub enum ServerMsg { State { master_volume: f32, playing: Vec<String>, sounds: Vec<RemoteSound> } }` tagged `"type":"state"`
  - `pub enum ClientMsg { Play { id: String }, StopAll, SetMasterVolume { value: f32 } }` tagged `"type"`, snake_case

- [ ] **Step 1: Write failing tests**

Bottom of `core/src/remote/auth.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7))
    }

    #[test]
    fn pin_format() {
        assert!(valid_pin_format("1234"));
        assert!(valid_pin_format("12345678"));
        assert!(!valid_pin_format("123"));
        assert!(!valid_pin_format("123456789"));
        assert!(!valid_pin_format("12a4"));
        assert!(!valid_pin_format(""));
    }

    #[test]
    fn pin_hash_round_trip() {
        let h = hash_pin("4321");
        assert!(verify_pin("4321", &h));
        assert!(!verify_pin("4322", &h));
        assert!(!verify_pin("4321", "garbage"));
    }

    #[test]
    fn token_verifies_with_same_secret_only() {
        let t = issue_token("secret-a");
        assert!(verify_token("secret-a", &t));
        assert!(!verify_token("secret-b", &t), "rotated secret invalidates");
        assert!(!verify_token("secret-a", "nonsense"));
        assert!(!verify_token("secret-a", ""));
        let mut tampered = t.clone();
        tampered.push('x');
        assert!(!verify_token("secret-a", &tampered));
    }

    #[test]
    fn lockout_blocks_after_max_failures_then_expires() {
        let mut l = Lockout::new(3, Duration::from_secs(600), Duration::from_secs(60));
        let t0 = Instant::now();
        l.record_failure(ip(), t0);
        l.record_failure(ip(), t0 + Duration::from_secs(1));
        assert!(!l.is_blocked(ip(), t0 + Duration::from_secs(2)));
        l.record_failure(ip(), t0 + Duration::from_secs(2));
        assert!(l.is_blocked(ip(), t0 + Duration::from_secs(3)));
        assert!(l.is_blocked(ip(), t0 + Duration::from_secs(61)));
        assert!(!l.is_blocked(ip(), t0 + Duration::from_secs(63)));
    }

    #[test]
    fn old_failures_fall_out_of_window() {
        let mut l = Lockout::new(3, Duration::from_secs(10), Duration::from_secs(60));
        let t0 = Instant::now();
        l.record_failure(ip(), t0);
        l.record_failure(ip(), t0 + Duration::from_secs(1));
        l.record_failure(ip(), t0 + Duration::from_secs(20)); // first two expired
        assert!(!l.is_blocked(ip(), t0 + Duration::from_secs(21)));
    }

    #[test]
    fn success_clears_failures_and_ips_are_independent() {
        let mut l = Lockout::new(2, Duration::from_secs(600), Duration::from_secs(60));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let t0 = Instant::now();
        l.record_failure(ip(), t0);
        l.record_success(ip());
        l.record_failure(ip(), t0);
        assert!(!l.is_blocked(ip(), t0));
        l.record_failure(other, t0);
        l.record_failure(other, t0);
        assert!(l.is_blocked(other, t0));
        assert!(!l.is_blocked(ip(), t0));
    }
}
```

Bottom of `core/src/remote/protocol.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_state_serializes_with_type_tag() {
        let m = ServerMsg::State {
            master_volume: 0.8,
            playing: vec!["u1".into()],
            sounds: vec![RemoteSound { id: "u1".into(), name: "Airhorn".into(), volume: 1.0, has_image: true }],
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(
            json,
            r#"{"type":"state","master_volume":0.8,"playing":["u1"],"sounds":[{"id":"u1","name":"Airhorn","volume":1.0,"has_image":true}]}"#
        );
    }

    #[test]
    fn client_messages_parse() {
        assert_eq!(serde_json::from_str::<ClientMsg>(r#"{"type":"play","id":"u1"}"#).unwrap(), ClientMsg::Play { id: "u1".into() });
        assert_eq!(serde_json::from_str::<ClientMsg>(r#"{"type":"stop_all"}"#).unwrap(), ClientMsg::StopAll);
        assert_eq!(serde_json::from_str::<ClientMsg>(r#"{"type":"set_master_volume","value":0.5}"#).unwrap(), ClientMsg::SetMasterVolume { value: 0.5 });
        assert!(serde_json::from_str::<ClientMsg>(r#"{"type":"explode"}"#).is_err());
    }
}
```

`core/src/remote/mod.rs` (for now):
```rust
pub mod auth;
pub mod protocol;
```

Add to `core/src/lib.rs`:
```rust
pub mod remote;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p soundboard-core remote`
Expected: compile error.

- [ ] **Step 3: Implement auth.rs**

Top of `core/src/remote/auth.rs`:
```rust
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn valid_pin_format(pin: &str) -> bool {
    (4..=8).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit())
}

pub fn hash_pin(pin: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(pin.as_bytes(), &salt).expect("argon2 hash").to_string()
}

pub fn verify_pin(pin: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pin.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

fn mac(secret: &str) -> HmacSha256 {
    HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length")
}

/// `<nonce>.<hmac(nonce)>`, both base64url without padding.
pub fn issue_token(secret: &str) -> String {
    let nonce: [u8; 16] = rand::random();
    let mut m = mac(secret);
    m.update(&nonce);
    format!("{}.{}", URL_SAFE_NO_PAD.encode(nonce), URL_SAFE_NO_PAD.encode(m.finalize().into_bytes()))
}

pub fn verify_token(secret: &str, token: &str) -> bool {
    let Some((n, s)) = token.split_once('.') else { return false };
    let (Ok(nonce), Ok(sig)) = (URL_SAFE_NO_PAD.decode(n), URL_SAFE_NO_PAD.decode(s)) else { return false };
    let mut m = mac(secret);
    m.update(&nonce);
    m.verify_slice(&sig).is_ok()
}

#[derive(Default)]
struct Entry {
    failures: Vec<Instant>,
    blocked_until: Option<Instant>,
}

/// Per-IP failure counter. Time is injected so tests do not sleep.
pub struct Lockout {
    max_failures: u32,
    window: Duration,
    block: Duration,
    entries: HashMap<IpAddr, Entry>,
}

impl Lockout {
    pub fn new(max_failures: u32, window: Duration, block: Duration) -> Self {
        Self { max_failures, window, block, entries: HashMap::new() }
    }

    /// 5 failures within 10 minutes block for 60 seconds.
    pub fn standard() -> Self {
        Self::new(5, Duration::from_secs(600), Duration::from_secs(60))
    }

    pub fn is_blocked(&mut self, ip: IpAddr, now: Instant) -> bool {
        match self.entries.get(&ip).and_then(|e| e.blocked_until) {
            Some(until) if now < until => true,
            _ => false,
        }
    }

    pub fn record_failure(&mut self, ip: IpAddr, now: Instant) {
        let (window, block, max) = (self.window, self.block, self.max_failures);
        let e = self.entries.entry(ip).or_default();
        e.failures.retain(|t| now.saturating_duration_since(*t) < window);
        e.failures.push(now);
        if e.failures.len() as u32 >= max {
            e.blocked_until = Some(now + block);
            e.failures.clear();
        }
    }

    pub fn record_success(&mut self, ip: IpAddr) {
        self.entries.remove(&ip);
    }
}
```

- [ ] **Step 4: Implement protocol.rs**

Top of `core/src/remote/protocol.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteSound {
    pub id: String,
    pub name: String,
    pub volume: f32,
    pub has_image: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    State { master_volume: f32, playing: Vec<String>, sounds: Vec<RemoteSound> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Play { id: String },
    StopAll,
    SetMasterVolume { value: f32 },
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p soundboard-core remote`
Expected: 8 passed (argon2 tests take a second or two in debug).

- [ ] **Step 6: Commit**

```bash
git add core/src/remote core/src/lib.rs
git commit -m "feat(core): remote PIN auth, tokens, lockout and protocol types"
```

---

### Task 10: Remote server (router, WebSocket hub, static files) with integration tests

**Files:**
- Modify: `core/src/remote/mod.rs`
- Create: `core/src/remote/page.html` (minimal, replaced in Task 11), `core/src/remote/manifest.json`, `core/src/remote/icon.svg`, `core/tests/remote_routes.rs`

**Interfaces:**
- Consumes: `auth`, `protocol`.
- Produces:
  - `pub trait RemoteHandler: Send + Sync + 'static { snapshot() -> ServerMsg; play(&str); stop_all(); set_master_volume(f32); image_path(&str) -> Option<PathBuf>; pin_hash() -> Option<String>; token_secret() -> String }`
  - `pub struct Hub` (Clone) with `new()`, `broadcast_state(&ServerMsg)`, `kick_all()`, `client_count() -> usize`, `set_on_clients_changed(Box<dyn Fn() + Send + Sync>)`
  - `pub fn router(handler: Arc<dyn RemoteHandler>, hub: Hub, lockout: Arc<Mutex<Lockout>>) -> axum::Router`
  - `pub async fn bind(port: u16) -> io::Result<(TcpListener, u16)>` (tries `port..port+11`)
  - `pub async fn serve(listener: TcpListener, router: Router) -> io::Result<()>`
  - Routes: `GET /`, `GET /manifest.json`, `GET /icon.svg`, `GET /img/{id}`, `GET /auth/status?token=`, `POST /auth`, `GET /ws?token=`
  - `pub const PORT_TRIES: u16 = 11`

- [ ] **Step 1: Static files**

`core/src/remote/page.html` (temporary; Task 11 writes the real page):
```html
<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Soundboard</title></head>
<body>Soundboard remote</body></html>
```

`core/src/remote/manifest.json`:
```json
{
  "name": "Soundboard",
  "short_name": "Soundboard",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#0a0a0b",
  "theme_color": "#0a0a0b",
  "icons": [{ "src": "/icon.svg", "sizes": "any", "type": "image/svg+xml" }]
}
```

`core/src/remote/icon.svg`:
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect width="64" height="64" rx="14" fill="#141416"/>
  <rect x="14" y="26" width="8" height="12" rx="2" fill="#ededef"/>
  <rect x="28" y="18" width="8" height="28" rx="2" fill="#f5b324"/>
  <rect x="42" y="22" width="8" height="20" rx="2" fill="#ededef"/>
</svg>
```

- [ ] **Step 2: Write failing integration tests**

`core/tests/remote_routes.rs`:
```rust
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
            sounds: vec![RemoteSound { id: "u1".into(), name: "Airhorn".into(), volume: 1.0, has_image: false }],
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
```

Also add unit tests to the bottom of `core/src/remote/mod.rs`:
```rust
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p soundboard-core --test remote_routes`
Expected: compile error, `router` not found.

- [ ] **Step 4: Implement mod.rs**

Replace `core/src/remote/mod.rs` contents (keep the tests module from Step 2 at the bottom):
```rust
//! LAN remote: axum server that serves the phone page and a WebSocket mirror of app state.

pub mod auth;
pub mod protocol;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
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

type ClientsChanged = Arc<Mutex<Option<Box<dyn Fn() + Send + Sync>>>>;

/// Fan-out of state to every connected phone, plus a client counter.
#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<HubEvent>,
    clients: Arc<AtomicUsize>,
    on_clients_changed: ClientsChanged,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self { tx, clients: Arc::new(AtomicUsize::new(0)), on_clients_changed: Arc::new(Mutex::new(None)) }
    }

    pub fn broadcast_state(&self, msg: &ServerMsg) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.tx.send(HubEvent::State(json));
        }
    }

    /// Closes every connected client; used after the PIN changes.
    pub fn kick_all(&self) {
        let _ = self.tx.send(HubEvent::Kick);
    }

    pub fn client_count(&self) -> usize {
        self.clients.load(Ordering::SeqCst)
    }

    pub fn set_on_clients_changed(&self, f: Box<dyn Fn() + Send + Sync>) {
        *self.on_clients_changed.lock().unwrap() = Some(f);
    }

    fn subscribe(&self) -> broadcast::Receiver<HubEvent> {
        self.tx.subscribe()
    }

    fn client_delta(&self, delta: isize) {
        if delta > 0 {
            self.clients.fetch_add(delta as usize, Ordering::SeqCst);
        } else {
            self.clients.fetch_sub((-delta) as usize, Ordering::SeqCst);
        }
        if let Some(f) = self.on_clients_changed.lock().unwrap().as_ref() {
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
    ctx.hub.client_delta(1);

    let send_snapshot = |sink: &mut futures_util::stream::SplitSink<WebSocket, Message>| {
        let json = serde_json::to_string(&ctx.handler.snapshot()).unwrap_or_default();
        async move { sink.send(Message::Text(json.into())).await }
    };

    if send_snapshot(&mut sink).await.is_err() {
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
                    if send_snapshot(&mut sink).await.is_err() { break; }
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
```

If the `send_snapshot` closure fights the borrow checker, replace both call sites with an inline block:
```rust
let json = serde_json::to_string(&ctx.handler.snapshot()).unwrap_or_default();
if sink.send(Message::Text(json.into())).await.is_err() { /* same handling */ }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p soundboard-core`
Expected: every unit test and all 8 integration tests pass. `sixth_wrong_attempt_is_429` takes a few seconds (argon2).

- [ ] **Step 6: Commit**

```bash
git add core
git commit -m "feat(core): axum remote server with WebSocket hub, auth routes and image serving"
```

---

### Task 11: Phone page

**Files:**
- Modify: `core/src/remote/page.html` (full replacement)
- Create: `core/examples/remote_dev.rs`

**Interfaces:**
- Consumes: routes and protocol from Task 10.
- Produces: a complete phone UI following the spec's design principles. Behavior: on load call `GET /auth/status`; if `pin_required && !token_valid` show the PIN screen, else open the WebSocket; render tiles from `state`; tap sends `play`; Stop All sends `stop_all`; master slider sends `set_master_volume`; reconnect with 0.5 s backoff doubling to 8 s; `navigator.vibrate(10)` on tap.

- [ ] **Step 1: Write the page**

`core/src/remote/page.html`:
```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no">
<meta name="theme-color" content="#0a0a0b">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
<link rel="manifest" href="/manifest.json">
<link rel="icon" href="/icon.svg">
<link rel="apple-touch-icon" href="/icon.svg">
<title>Soundboard</title>
<style>
:root{--bg:#0a0a0b;--surface:#141416;--surface-2:#1c1c1f;--border:#26262a;--text:#ededef;--dim:#8a8a93;--mute:#55555c;--accent:#f5b324;--danger:#ef4444;--radius:8px}
*{box-sizing:border-box;margin:0;padding:0;-webkit-tap-highlight-color:transparent}
html,body{height:100%;background:var(--bg);color:var(--text);font:13px/1.3 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;user-select:none;-webkit-user-select:none;touch-action:manipulation;overscroll-behavior:none}
button{font:inherit;color:inherit;cursor:pointer}
#app{display:flex;flex-direction:column;height:100%;padding-top:env(safe-area-inset-top)}
header{display:flex;align-items:center;gap:12px;height:48px;padding:0 12px;border-bottom:1px solid var(--border);flex-shrink:0}
.dot{width:8px;height:8px;border-radius:50%;background:var(--mute);flex-shrink:0}
.dot.on{background:var(--accent)}
input[type=range]{flex:1;-webkit-appearance:none;appearance:none;height:28px;background:transparent}
input[type=range]::-webkit-slider-runnable-track{height:2px;background:var(--border)}
input[type=range]::-webkit-slider-thumb{-webkit-appearance:none;width:18px;height:18px;margin-top:-8px;border-radius:50%;background:var(--text)}
main{flex:1;overflow-y:auto;padding:8px}
.grid{display:grid;grid-template-columns:repeat(3,1fr);gap:8px}
@media (min-width:600px){.grid{grid-template-columns:repeat(auto-fill,minmax(140px,1fr))}}
.tile{position:relative;aspect-ratio:1;border:0;border-radius:var(--radius);background:var(--surface-2);overflow:hidden;text-align:left;padding:0;transition:transform .08s}
.tile img{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;pointer-events:none}
.tile .name{position:absolute;left:0;right:0;bottom:0;padding:8px;font-weight:500;font-size:13px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.tile.img .name{padding-top:24px;background:linear-gradient(to top,rgba(0,0,0,.72),transparent)}
.tile.playing{box-shadow:0 0 0 2px var(--accent)}
.tile:active{transform:scale(.97)}
footer{flex-shrink:0;padding:8px 8px calc(8px + env(safe-area-inset-bottom))}
.stop{width:100%;height:44px;border:1px solid var(--danger);border-radius:var(--radius);background:transparent;color:var(--danger);font-weight:500}
.stop.on{background:var(--danger);color:#fff}
.empty{color:var(--dim);text-align:center;padding:48px 16px}
#pin{position:fixed;inset:0;background:var(--bg);display:none;flex-direction:column;align-items:center;justify-content:center;gap:20px}
#pin.show{display:flex}
.dots{display:flex;gap:10px;height:12px}
.dots i{width:12px;height:12px;border-radius:50%;border:1px solid var(--dim)}
.dots i.f{background:var(--text);border-color:var(--text)}
.pad{display:grid;grid-template-columns:repeat(3,72px);gap:10px}
.pad button{height:56px;border:1px solid var(--border);border-radius:var(--radius);background:var(--surface);font-size:20px}
.pad button:active{background:var(--surface-2)}
.pad button.ok{color:var(--accent);border-color:var(--accent)}
#pinmsg{color:var(--danger);height:16px;font-size:12px}
.hidden{display:none}
</style>
</head>
<body>
<div id="app">
  <header>
    <span id="dot" class="dot" title="Connection"></span>
    <input id="master" type="range" min="0" max="1" step="0.01" value="0.8" aria-label="Master volume">
  </header>
  <main>
    <div id="grid" class="grid"></div>
    <div id="empty" class="empty hidden">No sounds yet. Add some on the desktop app.</div>
  </main>
  <footer><button id="stop" class="stop" type="button">Stop all</button></footer>
</div>
<div id="pin">
  <div class="dots" id="dots"></div>
  <div id="pinmsg"></div>
  <div class="pad" id="pad"></div>
</div>
<script>
(() => {
  const $ = (id) => document.getElementById(id);
  const grid = $("grid"), empty = $("empty"), master = $("master"), stop = $("stop"), dot = $("dot");
  const pinScreen = $("pin"), dots = $("dots"), pad = $("pad"), pinMsg = $("pinmsg");
  const TOKEN_KEY = "sb_token";
  let ws = null, backoff = 500, state = null, pinBuf = "", masterDragging = false;

  const token = () => localStorage.getItem(TOKEN_KEY);
  const send = (msg) => { if (ws && ws.readyState === 1) ws.send(JSON.stringify(msg)); };
  const buzz = () => { if (navigator.vibrate) navigator.vibrate(10); };
  const nextBackoff = () => { const b = backoff; backoff = Math.min(backoff * 2, 8000); return b; };

  function render() {
    if (!state) return;
    if (!masterDragging) master.value = state.master_volume;
    empty.classList.toggle("hidden", state.sounds.length > 0);
    const playing = new Set(state.playing);
    stop.classList.toggle("on", playing.size > 0);
    const seen = new Set();
    for (const s of state.sounds) {
      seen.add(s.id);
      let el = grid.querySelector(`[data-id="${s.id}"]`);
      if (!el) {
        el = document.createElement("button");
        el.type = "button";
        el.className = "tile";
        el.dataset.id = s.id;
        el.addEventListener("click", () => { el.classList.add("playing"); buzz(); send({ type: "play", id: s.id }); });
        const name = document.createElement("span");
        name.className = "name";
        el.appendChild(name);
        grid.appendChild(el);
      }
      const wantImg = s.has_image ? `/img/${s.id}` : "";
      let img = el.querySelector("img");
      if (wantImg && !img) { img = document.createElement("img"); img.alt = ""; el.prepend(img); }
      if (!wantImg && img) img.remove();
      if (img && img.getAttribute("src") !== wantImg) img.src = wantImg;
      el.querySelector(".name").textContent = s.name;
      el.setAttribute("aria-label", s.name);
      el.classList.toggle("img", s.has_image);
      el.classList.toggle("playing", playing.has(s.id));
      grid.appendChild(el); // re-append keeps server order
    }
    for (const el of [...grid.children]) if (!seen.has(el.dataset.id)) el.remove();
  }

  async function status() {
    const t = token();
    const r = await fetch(`/auth/status${t ? "?token=" + encodeURIComponent(t) : ""}`, { cache: "no-store" });
    if (!r.ok) throw new Error("status " + r.status);
    return r.json();
  }

  async function boot() {
    try {
      const s = await status();
      if (s.pin_required && !s.token_valid) { localStorage.removeItem(TOKEN_KEY); showPin(); return; }
      connect();
    } catch {
      dot.classList.remove("on");
      setTimeout(boot, nextBackoff());
    }
  }

  function connect() {
    const t = token();
    const proto = location.protocol === "https:" ? "wss" : "ws";
    ws = new WebSocket(`${proto}://${location.host}/ws${t ? "?token=" + encodeURIComponent(t) : ""}`);
    ws.onopen = () => { backoff = 500; dot.classList.add("on"); pinScreen.classList.remove("show"); };
    ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.type === "state") { state = m; render(); } };
    ws.onclose = () => { dot.classList.remove("on"); ws = null; setTimeout(boot, nextBackoff()); };
    ws.onerror = () => { if (ws) ws.close(); };
  }

  function drawDots() {
    dots.innerHTML = "";
    for (let i = 0; i < Math.max(4, pinBuf.length); i++) {
      const d = document.createElement("i");
      if (i < pinBuf.length) d.className = "f";
      dots.appendChild(d);
    }
  }

  function showPin() { pinBuf = ""; pinMsg.textContent = ""; drawDots(); pinScreen.classList.add("show"); }

  async function submitPin() {
    let r = null;
    try {
      r = await fetch("/auth", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ pin: pinBuf }) });
    } catch { /* network */ }
    if (r && r.ok) {
      const { token: t } = await r.json();
      localStorage.setItem(TOKEN_KEY, t);
      pinScreen.classList.remove("show");
      backoff = 500;
      connect();
      return;
    }
    pinMsg.textContent = r && r.status === 429 ? "Too many attempts. Wait a minute." : r ? "Wrong PIN" : "No connection";
    pinBuf = "";
    drawDots();
  }

  for (const k of ["1","2","3","4","5","6","7","8","9","⌫","0","OK"]) {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = k;
    if (k === "OK") b.className = "ok";
    b.addEventListener("click", () => {
      buzz();
      if (k === "⌫") pinBuf = pinBuf.slice(0, -1);
      else if (k === "OK") { if (pinBuf.length >= 4) submitPin(); return; }
      else if (pinBuf.length < 8) pinBuf += k;
      pinMsg.textContent = "";
      drawDots();
    });
    pad.appendChild(b);
  }

  master.addEventListener("pointerdown", () => { masterDragging = true; });
  master.addEventListener("pointerup", () => { masterDragging = false; });
  master.addEventListener("input", () => send({ type: "set_master_volume", value: Number(master.value) }));
  stop.addEventListener("click", () => { buzz(); send({ type: "stop_all" }); });
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden && !ws) { backoff = 500; boot(); }
  });
  boot();
})();
</script>
</body>
</html>
```

- [ ] **Step 2: Dev harness so the page can be tested without Tauri**

`core/examples/remote_dev.rs`:
```rust
//! Serves the phone page with fake sounds on port 7331 (or the next free one).
//! cargo run -p soundboard-core --example remote_dev          -> no PIN
//! cargo run -p soundboard-core --example remote_dev 1234     -> PIN 1234
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use soundboard_core::remote::auth::{hash_pin, Lockout};
use soundboard_core::remote::protocol::{RemoteSound, ServerMsg};
use soundboard_core::remote::{bind, router, serve, Hub, RemoteHandler};

struct Dev {
    playing: Arc<Mutex<Vec<String>>>,
    hub: Hub,
    pin_hash: Option<String>,
}

fn snapshot(playing: &[String]) -> ServerMsg {
    ServerMsg::State {
        master_volume: 0.8,
        playing: playing.to_vec(),
        sounds: (1..=7)
            .map(|i| RemoteSound { id: format!("s{i}"), name: format!("Sound {i}"), volume: 1.0, has_image: false })
            .collect(),
    }
}

impl RemoteHandler for Dev {
    fn snapshot(&self) -> ServerMsg {
        snapshot(&self.playing.lock().unwrap())
    }
    fn play(&self, id: &str) {
        println!("play {id}");
        self.playing.lock().unwrap().push(id.to_string());
        self.hub.broadcast_state(&self.snapshot());
        let (playing, hub, id) = (self.playing.clone(), self.hub.clone(), id.to_string());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let snap = {
                let mut p = playing.lock().unwrap();
                if let Some(i) = p.iter().position(|x| *x == id) {
                    p.remove(i);
                }
                snapshot(&p)
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
        hub: hub.clone(),
        pin_hash: std::env::args().nth(1).map(|p| hash_pin(&p)),
    });
    let app = router(dev, hub, Arc::new(Mutex::new(Lockout::standard())));
    let (listener, port) = bind(7331).await.expect("bind");
    println!("open http://localhost:{port}  (phone: http://<your LAN ip>:{port})");
    serve(listener, app).await.expect("serve");
}
```

- [ ] **Step 3: Verify in a browser**

Run: `cargo test -p soundboard-core` (page still compiles into the binary, routes still pass).

Run in background: `cargo run -p soundboard-core --example remote_dev 1234`

Open `http://localhost:7331` in the built-in browser with a mobile viewport (375×812):
- PIN screen with four hollow dots and a 3×4 keypad appears. Wrong PIN shows "Wrong PIN" in red. `1234` then OK shows the grid.
- 7 flat tiles, 3 columns, names bottom-left. Tapping one shows the amber ring for about a second and prints `play sN` in the terminal.
- Stop all turns solid red while a tile is playing.
- Reload: no PIN prompt (token stored).
- Stop the server: dot turns gray; start it again: dot turns amber within 8 s.

Stop the example.

- [ ] **Step 4: Commit**

```bash
git add core/src/remote/page.html core/examples/remote_dev.rs
git commit -m "feat(core): phone remote page with PIN screen and reconnect"
```

---
### Task 12: Tauri app crate: state, commands, hotkey and remote glue

**Files:**
- Create: `src-tauri/src/state.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/hotkeys_glue.rs`, `src-tauri/src/remote_glue.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: everything from core.
- Produces (consumed by the React UI through `invoke`):
  - Event `state-changed` with payload `UiState`; event `toast` with payload `{ level: "info"|"warn"|"error", message: string }`.
  - `UiState { master_volume: f32, devices: { speakers, virtual_mic, available: DeviceInfo[] }, stop_all_hotkey: string, remote: { port: u16|null, error: string|null, pin_set: bool, addresses: string[], clients: usize }, sounds: UiSound[], playing: string[], startup_notices: string[] }`
  - `UiSound { id, name, volume, hotkey: string|null, image_path: string|null (absolute), missing_file: bool }`
  - Commands (JS arg names in camelCase): `get_state`, `play(id)`, `stop_all`, `set_master_volume(value)`, `set_sound_volume(id, value)`, `add_sounds(paths) -> string[]` (rejection messages), `rename_sound(id, name)`, `delete_sound(id)`, `set_sound_image(id, path)`, `clear_sound_image(id)`, `reorder_sounds(ids)`, `set_hotkey(id, accelerator)`, `clear_hotkey(id)`, `set_stop_all_hotkey(accelerator)`, `set_devices(speakers, virtualMic)`, `test_tone(device)`, `set_remote_port(port)`, `set_pin(pin)`, `restart_remote`.

No automated tests in this crate (everything testable is in core). Verification is `cargo check` plus a dev run.

- [ ] **Step 1: state.rs**

```rust
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use soundboard_core::audio::{DeviceInfo, Engine, Outputs, RodioBackend};
use soundboard_core::config::{self, Config};
use soundboard_core::library::Library;
use soundboard_core::remote::protocol::{RemoteSound, ServerMsg};
use soundboard_core::remote::Hub;
use tauri::{AppHandle, Emitter};

pub const CACHE_BYTES: usize = 200 * 1024 * 1024;

pub struct App {
    pub config: Config,
    pub engine: Engine<RodioBackend>,
    pub library: Library,
    pub remote_port: Option<u16>,
    pub remote_error: Option<String>,
    /// Device names already reported as missing this session.
    pub warned_devices: HashSet<String>,
    /// Shown once as toasts when the UI first loads.
    pub startup_notices: Vec<String>,
}

pub type Shared = Arc<Mutex<App>>;

/// Managed Tauri state.
pub struct Ctx {
    pub shared: Shared,
    pub hub: Hub,
    pub remote: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

pub fn lock(shared: &Shared) -> MutexGuard<'_, App> {
    shared.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Serialize, Clone)]
pub struct Toast {
    pub level: String,
    pub message: String,
}

impl Toast {
    pub fn warn(message: impl Into<String>) -> Self {
        Self { level: "warn".into(), message: message.into() }
    }
}

#[derive(Serialize, Clone)]
pub struct UiSound {
    pub id: String,
    pub name: String,
    pub volume: f32,
    pub hotkey: Option<String>,
    pub image_path: Option<String>,
    pub missing_file: bool,
}

#[derive(Serialize, Clone)]
pub struct UiDevices {
    pub speakers: Option<String>,
    pub virtual_mic: Option<String>,
    pub available: Vec<DeviceInfo>,
}

#[derive(Serialize, Clone)]
pub struct UiRemote {
    pub port: Option<u16>,
    pub error: Option<String>,
    pub pin_set: bool,
    pub addresses: Vec<String>,
    pub clients: usize,
}

#[derive(Serialize, Clone)]
pub struct UiState {
    pub master_volume: f32,
    pub devices: UiDevices,
    pub stop_all_hotkey: String,
    pub remote: UiRemote,
    pub sounds: Vec<UiSound>,
    pub playing: Vec<String>,
    pub startup_notices: Vec<String>,
}

impl App {
    pub fn outputs(&self) -> Outputs {
        Outputs { speakers: self.config.devices.speakers.clone(), virtual_mic: self.config.devices.virtual_mic.clone() }
    }

    pub fn save(&self) {
        if let Err(e) = config::save(&self.library.config_path(), &self.config) {
            log::error!("config save failed: {e}");
        }
    }

    /// Plays a sound; returns toasts for device fallbacks. Error string is user-facing.
    pub fn play(&mut self, id: &str) -> Result<Vec<Toast>, String> {
        let sound = self.config.sound(id).cloned().ok_or_else(|| "That sound no longer exists".to_string())?;
        let path = self.library.sound_path(&sound);
        if !path.exists() {
            return Err(format!("The file for \"{}\" is missing", sound.name));
        }
        let outcome = self
            .engine
            .play_file(&sound.id, sound.volume, &path)
            .map_err(|e| format!("Could not play \"{}\": {e}", sound.name))?;
        let mut toasts = Vec::new();
        for dev in outcome.fallbacks {
            if self.warned_devices.insert(dev.clone()) {
                toasts.push(Toast::warn(format!("Output device \"{dev}\" not found, using the default device")));
            }
        }
        Ok(toasts)
    }

    pub fn ui_state(&self, hub: &Hub) -> UiState {
        UiState {
            master_volume: self.config.master_volume,
            devices: UiDevices {
                speakers: self.config.devices.speakers.clone(),
                virtual_mic: self.config.devices.virtual_mic.clone(),
                available: self.engine.devices(),
            },
            stop_all_hotkey: self.config.stop_all_hotkey.clone(),
            remote: UiRemote {
                port: self.remote_port,
                error: self.remote_error.clone(),
                pin_set: self.config.remote.pin_hash.is_some(),
                addresses: lan_addresses(),
                clients: hub.client_count(),
            },
            sounds: self
                .config
                .sounds
                .iter()
                .map(|s| UiSound {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    volume: s.volume,
                    hotkey: s.hotkey.clone(),
                    image_path: self.library.image_path(s).map(|p| p.to_string_lossy().into_owned()),
                    missing_file: !self.library.sound_path(s).exists(),
                })
                .collect(),
            playing: self.engine.playing(),
            startup_notices: self.startup_notices.clone(),
        }
    }

    pub fn remote_snapshot(&self) -> ServerMsg {
        ServerMsg::State {
            master_volume: self.config.master_volume,
            playing: self.engine.playing(),
            sounds: self
                .config
                .sounds
                .iter()
                .map(|s| RemoteSound { id: s.id.clone(), name: s.name.clone(), volume: s.volume, has_image: s.image.is_some() })
                .collect(),
        }
    }
}

/// Non-loopback, non-link-local IPv4 addresses of this machine.
pub fn lan_addresses() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .map(|list| {
            list.into_iter()
                .filter_map(|(_, ip)| match ip {
                    IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() => Some(v4.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Pushes current state to the desktop UI and every phone. Never call while holding the App lock.
pub fn emit_state(app: &AppHandle, ctx: &Ctx) {
    let (ui, snap) = {
        let a = lock(&ctx.shared);
        (a.ui_state(&ctx.hub), a.remote_snapshot())
    };
    if let Err(e) = app.emit("state-changed", &ui) {
        log::warn!("emit state failed: {e}");
    }
    ctx.hub.broadcast_state(&snap);
}

pub fn emit_toast(app: &AppHandle, level: &str, message: impl Into<String>) {
    let _ = app.emit("toast", Toast { level: level.into(), message: message.into() });
}
```

- [ ] **Step 2: hotkeys_glue.rs**

```rust
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
```

- [ ] **Step 3: remote_glue.rs**

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use soundboard_core::remote::auth::Lockout;
use soundboard_core::remote::protocol::ServerMsg;
use soundboard_core::remote::{self, RemoteHandler};
use tauri::{AppHandle, Manager};

use crate::state::{emit_state, emit_toast, lock, Ctx};

struct Handler {
    app: AppHandle,
}

impl RemoteHandler for Handler {
    fn snapshot(&self) -> ServerMsg {
        lock(&self.app.state::<Ctx>().shared).remote_snapshot()
    }

    fn play(&self, id: &str) {
        let ctx = self.app.state::<Ctx>();
        let result = lock(&ctx.shared).play(id);
        match result {
            Ok(toasts) => {
                for t in toasts {
                    emit_toast(&self.app, &t.level, t.message);
                }
            }
            Err(e) => emit_toast(&self.app, "error", e),
        }
        emit_state(&self.app, &ctx);
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
            a.save();
        }
        emit_state(&self.app, &ctx);
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
    if let Some(h) = ctx.remote.lock().unwrap().take() {
        h.abort();
    }
    let port = lock(&ctx.shared).config.remote.port;
    let handler: Arc<dyn RemoteHandler> = Arc::new(Handler { app: app.clone() });
    let router = remote::router(handler, ctx.hub.clone(), Arc::new(Mutex::new(Lockout::standard())));

    let app2 = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let ctx = app2.state::<Ctx>();
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
```

- [ ] **Step 4: commands.rs**

```rust
use std::path::Path;

use soundboard_core::audio::DeviceTarget;
use soundboard_core::config::{self, Devices};
use soundboard_core::hotkeys::{self, Owner};
use soundboard_core::remote::auth;
use tauri::{AppHandle, State};

use crate::state::{emit_state, emit_toast, lock, App, Ctx, UiState};
use crate::{hotkeys_glue, remote_glue};

/// Lock, mutate, unlock. The caller emits state afterwards.
fn mutate<T>(ctx: &Ctx, f: impl FnOnce(&mut App) -> Result<T, String>) -> Result<T, String> {
    let mut app = lock(&ctx.shared);
    f(&mut app)
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

#[tauri::command]
pub fn play(id: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let toasts = mutate(&ctx, |a| a.play(&id))?;
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
        a.save();
    }
    emit_state(&app, &ctx);
}

#[tauri::command]
pub fn set_sound_volume(id: String, value: f32, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        let s = a.config.sound_mut(&id).ok_or("That sound no longer exists")?;
        s.volume = value.clamp(0.0, 1.0);
        a.engine.set_sound_volume(&id, value);
        a.save();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

/// Imports each path; returns one message per rejected file.
#[tauri::command]
pub fn add_sounds(paths: Vec<String>, app: AppHandle, ctx: State<'_, Ctx>) -> Vec<String> {
    let mut rejected = Vec::new();
    {
        let mut a = lock(&ctx.shared);
        for p in &paths {
            match a.library.import_sound(Path::new(p)) {
                Ok(s) => a.config.sounds.push(s),
                Err(e) => rejected.push(e.to_string()),
            }
        }
        a.save();
    }
    emit_state(&app, &ctx);
    rejected
}

#[tauri::command]
pub fn rename_sound(id: String, name: String, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    mutate(&ctx, |a| {
        a.config.sound_mut(&id).ok_or("That sound no longer exists")?.name = name;
        a.save();
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
        a.save();
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
        a.save();
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
        a.save();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn reorder_sounds(ids: Vec<String>, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    mutate(&ctx, |a| {
        if ids.len() != a.config.sounds.len() || !ids.iter().all(|id| a.config.sound(id).is_some()) {
            return Err("The sound list changed, try again".into());
        }
        let mut old = std::mem::take(&mut a.config.sounds);
        for id in &ids {
            if let Some(i) = old.iter().position(|s| &s.id == id) {
                a.config.sounds.push(old.remove(i));
            }
        }
        a.save();
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
        a.save();
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
        a.save();
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
        a.save();
        Ok(())
    })?;
    rebind_hotkeys(&app);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn set_devices(speakers: Option<String>, virtual_mic: Option<String>, app: AppHandle, ctx: State<'_, Ctx>) {
    {
        let mut a = lock(&ctx.shared);
        a.config.devices = Devices {
            speakers: speakers.filter(|s| !s.is_empty()),
            virtual_mic: virtual_mic.filter(|s| !s.is_empty()),
        };
        let outputs = a.outputs();
        a.engine.set_outputs(outputs);
        a.warned_devices.clear();
        a.save();
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
        a.save();
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
        a.save();
    }
    ctx.hub.kick_all();
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn restart_remote(app: AppHandle) {
    remote_glue::start(&app);
}
```

- [ ] **Step 5: lib.rs wiring**

Replace `src-tauri/src/lib.rs`:
```rust
mod commands;
mod hotkeys_glue;
mod remote_glue;
mod state;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use soundboard_core::audio::{Engine, RodioBackend};
use soundboard_core::config::{self, LoadOutcome};
use soundboard_core::library::Library;
use soundboard_core::remote::Hub;
use tauri::{AppHandle, Manager};

use state::{emit_state, lock, App, Ctx, Shared, CACHE_BYTES};

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
            let engine = Engine::new(RodioBackend::new(), outputs, cfg.master_volume, CACHE_BYTES);
            let shared: Shared = Arc::new(Mutex::new(App {
                config: cfg,
                engine,
                library,
                remote_port: None,
                remote_error: None,
                warned_devices: HashSet::new(),
                startup_notices: notices,
            }));
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
            commands::test_tone,
            commands::set_remote_port,
            commands::set_pin,
            commands::restart_remote,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Every 100 ms drop finished sinks and push state when the playing set changed.
fn spawn_reaper(app: AppHandle) {
    std::thread::Builder::new()
        .name("reaper".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let changed = {
                let ctx = app.state::<Ctx>();
                let mut a = lock(&ctx.shared);
                a.engine.reap()
            };
            if changed {
                let ctx = app.state::<Ctx>();
                emit_state(&app, &ctx);
            }
        })
        .expect("spawn reaper");
}
```

- [ ] **Step 6: Check and dev-run**

Run: `cargo check --workspace`
Expected: no errors. Fix any borrow or trait-import errors inline (common ones: add `use tauri::Manager;` where `.state()` is used; `use tauri::Emitter;` where `.emit()` is used).

Run in background: `npm run tauri dev`
Expected: window opens. In the webview devtools console (right-click, Inspect, or F12):
```js
const { invoke } = window.__TAURI__ ?? await import("@tauri-apps/api/core");
```
If `window.__TAURI__` is undefined, skip the console check; the UI tasks verify commands. Confirm instead that `%APPDATA%\co.carbonweb.soundboard\config.json` now exists with `"version": 1`, and that the remote page loads at `http://localhost:7331` in a browser (serving the Task 11 page with an empty grid).

Stop the dev process.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src
git commit -m "feat(app): wire core to Tauri commands, events, hotkeys and remote server"
```

---

### Task 13: Frontend foundation: API, accelerator helpers, store, shell, top bar, toasts

**Files:**
- Create: `src/lib/api.ts`, `src/lib/accel.ts`, `src/lib/accel.test.ts`, `src/lib/cx.ts`, `src/lib/store.ts`, `src/lib/store.test.ts`, `src/lib/actions.ts`, `src/components/TopBar.tsx`, `src/components/Toasts.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: Tauri commands and events from Task 12.
- Produces:
  - `api` object in `api.ts` with one method per command plus `onState(cb)`, `onToast(cb)`; types `UiState`, `UiSound`, `UiRemote`, `UiDevices`, `DeviceInfo`, `Toast`.
  - `accelFromEvent(e: KeyLike): string | null`, `displayAccel(accel): string` in `accel.ts`.
  - `cx(...classes)` in `cx.ts`.
  - `useStore` (zustand) with `state`, `panel`, `toasts`, `optimistic`, `init()`, `setPanel`, `togglePanel`, `toast(level, msg)`, `dismissToast(id)`, `play(id)`, `run(promise)`.
  - `pickAndAddSounds()`, `addPaths(paths)`, `pickImageFor(id)` in `actions.ts`.

- [ ] **Step 1: Write failing tests**

`src/lib/accel.test.ts`:
```ts
import { accelFromEvent, displayAccel } from "./accel";

const ev = (code: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
  ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, code, ...mods,
});

describe("accelFromEvent", () => {
  it("joins modifiers in fixed order with the code", () => {
    expect(accelFromEvent(ev("Digit1", { shiftKey: true, ctrlKey: true }))).toBe("Ctrl+Shift+Digit1");
    expect(accelFromEvent(ev("KeyZ", { metaKey: true, altKey: true }))).toBe("Alt+Super+KeyZ");
  });
  it("ignores bare modifier presses", () => {
    expect(accelFromEvent(ev("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(accelFromEvent(ev("ShiftRight", { shiftKey: true }))).toBeNull();
  });
  it("requires a modifier except for F-keys", () => {
    expect(accelFromEvent(ev("KeyA"))).toBeNull();
    expect(accelFromEvent(ev("F9"))).toBe("F9");
    expect(accelFromEvent(ev("F24"))).toBe("F24");
  });
  it("returns null for empty code", () => {
    expect(accelFromEvent(ev(""))).toBeNull();
  });
});

describe("displayAccel", () => {
  it("shortens key codes", () => {
    expect(displayAccel("Ctrl+Shift+Digit1")).toBe("Ctrl+Shift+1");
    expect(displayAccel("Alt+KeyQ")).toBe("Alt+Q");
    expect(displayAccel("Ctrl+Shift+F12")).toBe("Ctrl+Shift+F12");
    expect(displayAccel("Ctrl+Escape")).toBe("Ctrl+Esc");
    expect(displayAccel("Ctrl+Numpad5")).toBe("Ctrl+Num5");
    expect(displayAccel("Ctrl+ArrowUp")).toBe("Ctrl+↑");
    expect(displayAccel("Ctrl+BracketLeft")).toBe("Ctrl+[");
  });
  it("handles empty", () => {
    expect(displayAccel(null)).toBe("");
    expect(displayAccel(undefined)).toBe("");
  });
});
```

`src/lib/store.test.ts`:
```ts
import { useStore } from "./store";

vi.mock("./api", () => ({
  api: {
    play: vi.fn(),
    getState: vi.fn(),
    onState: vi.fn(),
    onToast: vi.fn(),
  },
}));

describe("store", () => {
  beforeEach(() => {
    useStore.setState({ toasts: [], panel: null, optimistic: new Set() });
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("keeps at most three toasts and auto-dismisses after 4s", () => {
    const { toast } = useStore.getState();
    toast("info", "a");
    toast("info", "b");
    toast("info", "c");
    toast("info", "d");
    expect(useStore.getState().toasts.map((t) => t.message)).toEqual(["b", "c", "d"]);
    vi.advanceTimersByTime(4000);
    expect(useStore.getState().toasts).toHaveLength(0);
  });

  it("run() turns a rejection into an error toast", async () => {
    await useStore.getState().run(Promise.reject("Port must be between 1024 and 65535"));
    expect(useStore.getState().toasts[0]).toMatchObject({ level: "error", message: "Port must be between 1024 and 65535" });
  });

  it("togglePanel opens and closes", () => {
    const { togglePanel } = useStore.getState();
    togglePanel("settings");
    expect(useStore.getState().panel).toBe("settings");
    togglePanel("settings");
    expect(useStore.getState().panel).toBeNull();
    togglePanel("settings");
    togglePanel("remote");
    expect(useStore.getState().panel).toBe("remote");
  });

  it("play() marks optimistic then clears", async () => {
    const { api } = await import("./api");
    (api.play as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    const p = useStore.getState().play("s1");
    expect(useStore.getState().optimistic.has("s1")).toBe(true);
    await p;
    vi.advanceTimersByTime(200);
    expect(useStore.getState().optimistic.has("s1")).toBe(false);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test`
Expected: failures, modules not found.

- [ ] **Step 3: api.ts**

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface DeviceInfo { name: string; is_default: boolean }
export interface UiSound {
  id: string;
  name: string;
  volume: number;
  hotkey: string | null;
  image_path: string | null;
  missing_file: boolean;
}
export interface UiDevices { speakers: string | null; virtual_mic: string | null; available: DeviceInfo[] }
export interface UiRemote { port: number | null; error: string | null; pin_set: boolean; addresses: string[]; clients: number }
export interface UiState {
  master_volume: number;
  devices: UiDevices;
  stop_all_hotkey: string;
  remote: UiRemote;
  sounds: UiSound[];
  playing: string[];
  startup_notices: string[];
}
export type ToastLevel = "info" | "warn" | "error";
export interface Toast { level: ToastLevel; message: string }

export const api = {
  getState: () => invoke<UiState>("get_state"),
  play: (id: string) => invoke<void>("play", { id }),
  stopAll: () => invoke<void>("stop_all"),
  setMasterVolume: (value: number) => invoke<void>("set_master_volume", { value }),
  setSoundVolume: (id: string, value: number) => invoke<void>("set_sound_volume", { id, value }),
  addSounds: (paths: string[]) => invoke<string[]>("add_sounds", { paths }),
  renameSound: (id: string, name: string) => invoke<void>("rename_sound", { id, name }),
  deleteSound: (id: string) => invoke<void>("delete_sound", { id }),
  setSoundImage: (id: string, path: string) => invoke<void>("set_sound_image", { id, path }),
  clearSoundImage: (id: string) => invoke<void>("clear_sound_image", { id }),
  reorderSounds: (ids: string[]) => invoke<void>("reorder_sounds", { ids }),
  setHotkey: (id: string, accelerator: string) => invoke<void>("set_hotkey", { id, accelerator }),
  clearHotkey: (id: string) => invoke<void>("clear_hotkey", { id }),
  setStopAllHotkey: (accelerator: string) => invoke<void>("set_stop_all_hotkey", { accelerator }),
  setDevices: (speakers: string | null, virtualMic: string | null) => invoke<void>("set_devices", { speakers, virtualMic }),
  testTone: (device: string | null) => invoke<void>("test_tone", { device }),
  setRemotePort: (port: number) => invoke<void>("set_remote_port", { port }),
  setPin: (pin: string | null) => invoke<void>("set_pin", { pin }),
  restartRemote: () => invoke<void>("restart_remote"),
  onState: (cb: (s: UiState) => void): Promise<UnlistenFn> => listen<UiState>("state-changed", (e) => cb(e.payload)),
  onToast: (cb: (t: Toast) => void): Promise<UnlistenFn> => listen<Toast>("toast", (e) => cb(e.payload)),
};
```

- [ ] **Step 4: accel.ts and cx.ts**

`src/lib/accel.ts`:
```ts
const MOD_CODES = new Set([
  "ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight",
  "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

export interface KeyLike {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  code: string;
}

const F_KEY = /^F([1-9]|1[0-9]|2[0-4])$/;

/** Builds the canonical accelerator for a keydown, or null if it is not bindable. */
export function accelFromEvent(e: KeyLike): string | null {
  if (!e.code || MOD_CODES.has(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (mods.length === 0 && !F_KEY.test(e.code)) return null;
  return [...mods, e.code].join("+");
}

const NAMES: Record<string, string> = {
  Escape: "Esc", Enter: "Enter", Space: "Space", Tab: "Tab", Backspace: "Bksp", Delete: "Del",
  Insert: "Ins", Home: "Home", End: "End", PageUp: "PgUp", PageDown: "PgDn",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→",
  Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\",
  Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/", Backquote: "`",
};

/** Human form of an accelerator for badges: Ctrl+Shift+Digit1 -> Ctrl+Shift+1 */
export function displayAccel(accel: string | null | undefined): string {
  if (!accel) return "";
  return accel
    .split("+")
    .map((p) => {
      if (/^Key[A-Z]$/.test(p)) return p[3];
      if (/^Digit[0-9]$/.test(p)) return p[5];
      if (p.startsWith("Numpad")) return "Num" + p.slice(6);
      return NAMES[p] ?? p;
    })
    .join("+");
}
```

`src/lib/cx.ts`:
```ts
export const cx = (...xs: Array<string | false | null | undefined>) => xs.filter(Boolean).join(" ");
```

- [ ] **Step 5: store.ts**

```ts
import { create } from "zustand";
import { api, type Toast, type ToastLevel, type UiState } from "./api";

export type Panel = "settings" | "remote" | null;
export interface ToastItem extends Toast { id: number }

interface Store {
  state: UiState | null;
  panel: Panel;
  toasts: ToastItem[];
  /** Sound ids shown as playing before Rust confirms. */
  optimistic: Set<string>;
  init: () => Promise<void>;
  setPanel: (p: Panel) => void;
  togglePanel: (p: Exclude<Panel, null>) => void;
  toast: (level: ToastLevel, message: string) => void;
  dismissToast: (id: number) => void;
  play: (id: string) => Promise<void>;
  /** Awaits a command; a rejection becomes an error toast. */
  run: (p: Promise<unknown>) => Promise<void>;
}

let nextToastId = 1;
let started = false;

export const useStore = create<Store>((set, get) => ({
  state: null,
  panel: null,
  toasts: [],
  optimistic: new Set(),

  init: async () => {
    if (started) return;
    started = true;
    await api.onState((s) => set({ state: s }));
    await api.onToast((t) => get().toast(t.level, t.message));
    const s = await api.getState();
    set({ state: s });
    s.startup_notices.forEach((n) => get().toast("warn", n));
  },

  setPanel: (panel) => set({ panel }),
  togglePanel: (p) => set((st) => ({ panel: st.panel === p ? null : p })),

  toast: (level, message) => {
    const id = nextToastId++;
    set((st) => ({ toasts: [...st.toasts, { id, level, message }].slice(-3) }));
    setTimeout(() => get().dismissToast(id), 4000);
  },
  dismissToast: (id) => set((st) => ({ toasts: st.toasts.filter((t) => t.id !== id) })),

  play: async (id) => {
    set((st) => ({ optimistic: new Set(st.optimistic).add(id) }));
    try {
      await api.play(id);
    } catch (e) {
      get().toast("error", String(e));
    } finally {
      setTimeout(() => {
        set((st) => {
          const n = new Set(st.optimistic);
          n.delete(id);
          return { optimistic: n };
        });
      }, 150);
    }
  },

  run: async (p) => {
    try {
      await p;
    } catch (e) {
      get().toast("error", String(e));
    }
  },
}));
```

- [ ] **Step 6: actions.ts**

```ts
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { useStore } from "./store";

const AUDIO = ["mp3", "wav", "ogg", "flac"];
const IMAGES = ["png", "jpg", "jpeg", "webp", "gif"];

/** Imports files; each rejected file becomes a warning toast. */
export async function addPaths(paths: string[]) {
  const { toast } = useStore.getState();
  try {
    const rejected = await api.addSounds(paths);
    rejected.forEach((r) => toast("warn", r));
  } catch (e) {
    toast("error", String(e));
  }
}

export async function pickAndAddSounds() {
  const picked = await open({ multiple: true, filters: [{ name: "Audio", extensions: AUDIO }] });
  if (!picked) return;
  await addPaths(Array.isArray(picked) ? picked : [picked]);
}

export async function pickImageFor(id: string) {
  const picked = await open({ multiple: false, filters: [{ name: "Image", extensions: IMAGES }] });
  if (!picked || Array.isArray(picked)) return;
  await useStore.getState().run(api.setSoundImage(id, picked));
}
```

- [ ] **Step 7: Toasts.tsx and TopBar.tsx**

`src/components/Toasts.tsx`:
```tsx
import { cx } from "../lib/cx";
import { useStore } from "../lib/store";

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-3 left-3 flex w-80 flex-col gap-2" role="status" aria-live="polite">
      {toasts.map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => dismiss(t.id)}
          className={cx(
            "pointer-events-auto rounded-md border-l-2 bg-surface px-3 py-2 text-left shadow-lg",
            t.level === "error" && "border-danger",
            t.level === "warn" && "border-accent",
            t.level === "info" && "border-dim",
          )}
        >
          {t.message}
        </button>
      ))}
    </div>
  );
}
```

`src/components/TopBar.tsx`:
```tsx
import { pickAndAddSounds } from "../lib/actions";
import { displayAccel } from "../lib/accel";
import { api } from "../lib/api";
import { cx } from "../lib/cx";
import { useStore } from "../lib/store";

function SpeakerGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M2 6h3l4-3v10l-4-3H2z" />
      <path d="M11 5.5a3 3 0 0 1 0 5" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  );
}

export function TopBar() {
  const state = useStore((s) => s.state)!;
  const run = useStore((s) => s.run);
  const panel = useStore((s) => s.panel);
  const togglePanel = useStore((s) => s.togglePanel);
  const playing = state.playing.length > 0;

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
      <span className="text-dim">Soundboard</span>
      <span className="text-mute tabular-nums">{state.sounds.length}</span>
      <div className="flex-1" />

      <label className="flex items-center gap-2 text-dim" title="Master volume">
        <SpeakerGlyph />
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={state.master_volume}
          aria-label="Master volume"
          onChange={(e) => run(api.setMasterVolume(Number(e.target.value)))}
          className="w-[120px]"
        />
      </label>

      <button
        type="button"
        onClick={() => run(api.stopAll())}
        title={`Stop all (${displayAccel(state.stop_all_hotkey)})`}
        className={cx("btn", playing ? "border-danger bg-danger text-white hover:bg-danger" : "border-danger text-danger")}
      >
        Stop
      </button>

      <button type="button" onClick={() => void pickAndAddSounds()} className="btn border-text bg-text text-bg hover:bg-text">
        Add
      </button>

      <button type="button" onClick={() => togglePanel("remote")} aria-pressed={panel === "remote"} className="btn relative">
        Remote
        {state.remote.clients > 0 && <span className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-accent" aria-label="Phone connected" />}
      </button>

      <button type="button" onClick={() => togglePanel("settings")} aria-pressed={panel === "settings"} className="btn">
        Settings
      </button>
    </header>
  );
}
```

- [ ] **Step 8: App.tsx shell**

```tsx
import { useEffect } from "react";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import { api } from "./lib/api";
import { useStore } from "./lib/store";

export default function App() {
  const init = useStore((s) => s.init);
  const state = useStore((s) => s.state);

  useEffect(() => {
    void init();
  }, [init]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (e.key === "Escape" && !typing) void api.stopAll();
    };
    const noMenu = (e: MouseEvent) => e.preventDefault();
    window.addEventListener("keydown", onKey);
    window.addEventListener("contextmenu", noMenu);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", noMenu);
    };
  }, []);

  if (!state) return <div className="h-full" />;

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <main className="min-w-0 flex-1 overflow-y-auto p-3" />
      </div>
      <Toasts />
    </div>
  );
}
```

- [ ] **Step 9: Run tests, typecheck, dev-run**

Run: `npm test`
Expected: accel 6 tests and store 4 tests pass.

Run: `npm run build`
Expected: no type errors.

Run in background: `npm run tauri dev`. Expected: top bar with "Soundboard 0", master slider, red-outlined Stop, white Add, Remote, Settings. Clicking Add opens a file dialog filtered to audio; picking an mp3 makes the count go to 1 (grid comes in Task 14). Stop it.

- [ ] **Step 10: Commit**

```bash
git add src
git commit -m "feat(ui): api wrapper, accelerator helpers, store, top bar and toasts"
```

---
### Task 14: Tile grid: tiles, reorder, context menu, dialogs, hotkey capture, drag-drop import

**Files:**
- Create: `src/components/Tile.tsx`, `src/components/Tile.test.tsx`, `src/components/TileGrid.tsx`, `src/components/ContextMenu.tsx`, `src/components/ContextMenu.test.tsx`, `src/components/Dialogs.tsx`, `src/components/HotkeyCapture.tsx`, `src/components/HotkeyCapture.test.tsx`, `src/components/EmptyState.tsx`, `src/components/Banner.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `api`, `useStore`, `accelFromEvent`, `displayAccel`, `addPaths`, `pickImageFor`, `pickAndAddSounds`.
- Produces: `<TileGrid />` (self-contained), `<Tile sound playing onPlay onVolume onContextMenu />`, `<ContextMenu x y items onClose />` with `items: { label: string; onSelect: () => void; danger?: boolean }[]`, `<ConfirmDialog title confirmLabel danger onConfirm onCancel />`, `<RenameDialog title initial onSubmit onCancel />`, `<HotkeyCapture title current onCapture onClear? onCancel />`.

- [ ] **Step 1: Write failing tests**

`src/components/Tile.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { DndContext } from "@dnd-kit/core";
import { SortableContext } from "@dnd-kit/sortable";
import { Tile } from "./Tile";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => "asset://" + p,
  invoke: vi.fn(),
}));

const sound = { id: "s1", name: "Airhorn", volume: 0.7, hotkey: "Ctrl+Shift+Digit1", image_path: null, missing_file: false };

function renderTile(over: Partial<typeof sound> = {}, playing = false, onPlay = vi.fn(), onVolume = vi.fn()) {
  const s = { ...sound, ...over };
  render(
    <DndContext>
      <SortableContext items={[s.id]}>
        <Tile sound={s} playing={playing} onPlay={onPlay} onVolume={onVolume} onContextMenu={() => {}} />
      </SortableContext>
    </DndContext>,
  );
  return { onPlay, onVolume };
}

describe("Tile", () => {
  it("shows name and shortened hotkey badge", () => {
    renderTile();
    expect(screen.getByText("Airhorn")).toBeInTheDocument();
    expect(screen.getByText("Ctrl+Shift+1")).toBeInTheDocument();
  });

  it("marks playing state on the button", () => {
    renderTile({}, true);
    expect(screen.getByRole("button", { name: "Airhorn" })).toHaveAttribute("aria-pressed", "true");
  });

  it("renders the image when present", () => {
    renderTile({ image_path: "C:/x/s1.png" });
    expect(document.querySelector("img")?.getAttribute("src")).toBe("asset://C:/x/s1.png");
  });

  it("flags a missing file", () => {
    renderTile({ missing_file: true });
    expect(screen.getByTitle("File missing")).toBeInTheDocument();
  });

  it("click plays, slider sets volume", () => {
    const { onPlay, onVolume } = renderTile();
    fireEvent.click(screen.getByRole("button", { name: "Airhorn" }));
    expect(onPlay).toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText("Airhorn volume"), { target: { value: "0.3" } });
    expect(onVolume).toHaveBeenCalledWith(0.3);
  });
});
```

`src/components/ContextMenu.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { ContextMenu } from "./ContextMenu";

describe("ContextMenu", () => {
  it("renders items and calls onSelect then onClose", () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<ContextMenu x={10} y={10} onClose={onClose} items={[{ label: "Rename", onSelect }, { label: "Delete", onSelect: vi.fn(), danger: true }]} />);
    fireEvent.click(screen.getByText("Rename"));
    expect(onSelect).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    expect(screen.getByText("Delete")).toHaveClass("text-danger");
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "A", onSelect: vi.fn() }]} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
```

`src/components/HotkeyCapture.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { HotkeyCapture } from "./HotkeyCapture";

describe("HotkeyCapture", () => {
  it("captures a combo and commits it", () => {
    const onCapture = vi.fn();
    render(<HotkeyCapture title="Hotkey" current={null} onCapture={onCapture} onCancel={vi.fn()} />);
    fireEvent.keyDown(window, { code: "Digit2", ctrlKey: true, shiftKey: true });
    expect(onCapture).toHaveBeenCalledWith("Ctrl+Shift+Digit2");
  });

  it("ignores a bare letter and shows the hint", () => {
    const onCapture = vi.fn();
    render(<HotkeyCapture title="Hotkey" current={null} onCapture={onCapture} onCancel={vi.fn()} />);
    fireEvent.keyDown(window, { code: "KeyA" });
    expect(onCapture).not.toHaveBeenCalled();
    expect(screen.getByText(/needs a modifier/i)).toBeInTheDocument();
  });

  it("plain Escape cancels, Clear calls onClear", () => {
    const onCancel = vi.fn();
    const onClear = vi.fn();
    render(<HotkeyCapture title="Hotkey" current="Alt+KeyQ" onCapture={vi.fn()} onClear={onClear} onCancel={onCancel} />);
    expect(screen.getByText("Alt+Q")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Clear"));
    expect(onClear).toHaveBeenCalled();
    fireEvent.keyDown(window, { code: "Escape", key: "Escape" });
    expect(onCancel).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test`
Expected: the three new files fail with module not found.

- [ ] **Step 3: Tile.tsx**

```tsx
import { convertFileSrc } from "@tauri-apps/api/core";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { MouseEvent } from "react";
import { displayAccel } from "../lib/accel";
import type { UiSound } from "../lib/api";
import { cx } from "../lib/cx";

interface Props {
  sound: UiSound;
  playing: boolean;
  onPlay: () => void;
  onVolume: (v: number) => void;
  onContextMenu: (e: MouseEvent) => void;
}

export function Tile({ sound, playing, onPlay, onVolume, onContextMenu }: Props) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: sound.id });
  const style = { transform: CSS.Transform.toString(transform), transition };
  const src = sound.image_path ? convertFileSrc(sound.image_path) : null;

  return (
    <div ref={setNodeRef} style={style} className={cx("group relative aspect-square", isDragging && "z-10 opacity-80")}>
      <button
        type="button"
        {...attributes}
        {...listeners}
        onClick={onPlay}
        onContextMenu={onContextMenu}
        aria-label={sound.name}
        aria-pressed={playing}
        className={cx(
          "block h-full w-full overflow-hidden rounded-md bg-surface-2 text-left outline-none ring-accent transition-shadow focus-visible:ring-2",
          playing && "ring-2",
        )}
      >
        {src && <img src={src} alt="" draggable={false} className="pointer-events-none absolute inset-0 h-full w-full object-cover" />}
        <span
          className={cx(
            "absolute inset-x-0 bottom-0 truncate p-2 text-[13px] font-medium leading-tight",
            src && "bg-gradient-to-t from-black/70 to-transparent pt-6",
          )}
        >
          {sound.name}
        </span>
        {sound.hotkey && (
          <span className="absolute right-1.5 top-1.5 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[11px] text-dim">
            {displayAccel(sound.hotkey)}
          </span>
        )}
        {sound.missing_file && (
          <span title="File missing" className="absolute left-1.5 top-1.5 rounded bg-black/60 px-1.5 py-0.5 font-mono text-[11px] text-danger">
            !
          </span>
        )}
      </button>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={sound.volume}
        aria-label={`${sound.name} volume`}
        onChange={(e) => onVolume(Number(e.target.value))}
        onPointerDown={(e) => e.stopPropagation()}
        className="absolute inset-x-2 bottom-7 opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100"
      />
    </div>
  );
}
```

- [ ] **Step 4: ContextMenu.tsx, Dialogs.tsx, HotkeyCapture.tsx, EmptyState.tsx, Banner.tsx**

`src/components/ContextMenu.tsx`:
```tsx
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { cx } from "../lib/cx";

export interface MenuItem { label: string; onSelect: () => void; danger?: boolean }

interface Props { x: number; y: number; items: MenuItem[]; onClose: () => void }

export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.min(x, window.innerWidth - r.width - 8),
      y: Math.min(y, window.innerHeight - r.height - 8),
    });
  }, [x, y]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    const onDown = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) onClose(); };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  return (
    <div ref={ref} role="menu" style={{ left: pos.x, top: pos.y }} className="fixed z-50 min-w-40 rounded-md border border-border bg-surface py-1 shadow-xl">
      {items.map((it) => (
        <button
          key={it.label}
          type="button"
          role="menuitem"
          onClick={() => { it.onSelect(); onClose(); }}
          className={cx("block w-full px-3 py-1.5 text-left hover:bg-surface-2", it.danger && "text-danger")}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}
```

`src/components/Dialogs.tsx`:
```tsx
import { useEffect, useRef, useState, type ReactNode } from "react";
import { cx } from "../lib/cx";

function Backdrop({ children, onCancel }: { children: ReactNode; onCancel: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); onCancel(); } };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCancel]);
  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50" onMouseDown={onCancel}>
      <div role="dialog" className="w-80 rounded-md border border-border bg-surface p-4 shadow-xl" onMouseDown={(e) => e.stopPropagation()}>
        {children}
      </div>
    </div>
  );
}

interface ConfirmProps { title: string; confirmLabel: string; danger?: boolean; onConfirm: () => void; onCancel: () => void }

export function ConfirmDialog({ title, confirmLabel, danger, onConfirm, onCancel }: ConfirmProps) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => ref.current?.focus(), []);
  return (
    <Backdrop onCancel={onCancel}>
      <p className="mb-4">{title}</p>
      <div className="flex justify-end gap-2">
        <button type="button" className="btn" onClick={onCancel}>Cancel</button>
        <button ref={ref} type="button" onClick={onConfirm} className={cx("btn", danger && "border-danger bg-danger text-white hover:bg-danger")}>
          {confirmLabel}
        </button>
      </div>
    </Backdrop>
  );
}

interface RenameProps { title: string; initial: string; onSubmit: (value: string) => void; onCancel: () => void }

export function RenameDialog({ title, initial, onSubmit, onCancel }: RenameProps) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { ref.current?.focus(); ref.current?.select(); }, []);
  const submit = () => { const v = value.trim(); if (v) onSubmit(v); };
  return (
    <Backdrop onCancel={onCancel}>
      <form onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <label className="label" htmlFor="rename">{title}</label>
        <input id="rename" ref={ref} className="field mb-4" value={value} onChange={(e) => setValue(e.target.value)} />
        <div className="flex justify-end gap-2">
          <button type="button" className="btn" onClick={onCancel}>Cancel</button>
          <button type="submit" className="btn border-text bg-text text-bg hover:bg-text" disabled={!value.trim()}>Save</button>
        </div>
      </form>
    </Backdrop>
  );
}
```

`src/components/HotkeyCapture.tsx`:
```tsx
import { useEffect, useState } from "react";
import { accelFromEvent, displayAccel } from "../lib/accel";

interface Props {
  title: string;
  current: string | null;
  onCapture: (accelerator: string) => void;
  onClear?: () => void;
  onCancel: () => void;
}

/** Captures the next key combination and commits it immediately. Plain Esc cancels. */
export function HotkeyCapture({ title, current, onCapture, onClear, onCancel }: Props) {
  const [hint, setHint] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const bare = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
      if (e.code === "Escape" && bare) { onCancel(); return; }
      const accel = accelFromEvent(e);
      if (accel) { onCapture(accel); return; }
      if (e.code && !/^(Control|Shift|Alt|Meta)(Left|Right)$/.test(e.code)) {
        setHint("That key needs a modifier such as Ctrl or Alt. F-keys may be used alone.");
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onCapture, onCancel]);

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50" onMouseDown={onCancel}>
      <div role="dialog" className="w-80 rounded-md border border-border bg-surface p-4 shadow-xl" onMouseDown={(e) => e.stopPropagation()}>
        <p className="mb-1 text-dim">{title}</p>
        <p className="mb-3 font-mono text-lg">{current ? displayAccel(current) : "Press keys…"}</p>
        <p className="mb-4 min-h-8 text-xs text-mute">{hint ?? "Press a key combination. Esc to cancel."}</p>
        <div className="flex justify-end gap-2">
          {onClear && <button type="button" className="btn border-danger text-danger" onClick={onClear}>Clear</button>}
          <button type="button" className="btn" onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
```

`src/components/EmptyState.tsx`:
```tsx
import { pickAndAddSounds } from "../lib/actions";
import { cx } from "../lib/cx";

export function EmptyState({ dragOver }: { dragOver: boolean }) {
  return (
    <div className={cx("flex h-full items-center justify-center rounded-md border border-dashed text-dim", dragOver ? "border-accent" : "border-border")}>
      <span>
        Drop audio files here, or{" "}
        <button type="button" className="underline underline-offset-2 hover:text-text" onClick={() => void pickAndAddSounds()}>
          Add
        </button>
        .
      </span>
    </div>
  );
}
```

`src/components/Banner.tsx`:
```tsx
import { useStore } from "../lib/store";

export function Banner() {
  const setPanel = useStore((s) => s.setPanel);
  return (
    <div className="mb-3 rounded-md border border-border bg-surface px-3 py-2 text-dim">
      No output device selected.{" "}
      <button type="button" className="text-text underline underline-offset-2" onClick={() => setPanel("settings")}>
        Open Settings
      </button>
    </div>
  );
}
```

- [ ] **Step 5: TileGrid.tsx**

```tsx
import { closestCenter, DndContext, PointerSensor, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { arrayMove, rectSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState } from "react";
import { addPaths, pickImageFor } from "../lib/actions";
import { api, type UiSound } from "../lib/api";
import { cx } from "../lib/cx";
import { useStore } from "../lib/store";
import { Banner } from "./Banner";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { ConfirmDialog, RenameDialog } from "./Dialogs";
import { EmptyState } from "./EmptyState";
import { HotkeyCapture } from "./HotkeyCapture";
import { Tile } from "./Tile";

type Menu = { sound: UiSound; x: number; y: number };

export function TileGrid() {
  const state = useStore((s) => s.state)!;
  const play = useStore((s) => s.play);
  const run = useStore((s) => s.run);
  const optimistic = useStore((s) => s.optimistic);

  const [menu, setMenu] = useState<Menu | null>(null);
  const [deleting, setDeleting] = useState<UiSound | null>(null);
  const [renaming, setRenaming] = useState<UiSound | null>(null);
  const [capturing, setCapturing] = useState<UiSound | null>(null);
  const [dragOver, setDragOver] = useState(false);

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") setDragOver(true);
      else if (e.payload.type === "leave") setDragOver(false);
      else if (e.payload.type === "drop") {
        setDragOver(false);
        void addPaths(e.payload.paths);
      }
    });
    return () => { void unlisten.then((f) => f()); };
  }, []);

  const onDragEnd = (ev: DragEndEvent) => {
    const { active, over } = ev;
    if (!over || active.id === over.id) return;
    const ids = state.sounds.map((s) => s.id);
    void run(api.reorderSounds(arrayMove(ids, ids.indexOf(String(active.id)), ids.indexOf(String(over.id)))));
  };

  const menuItems = (s: UiSound): MenuItem[] => [
    { label: "Rename", onSelect: () => setRenaming(s) },
    { label: s.image_path ? "Change image" : "Set image", onSelect: () => void pickImageFor(s.id) },
    ...(s.image_path ? [{ label: "Remove image", onSelect: () => void run(api.clearSoundImage(s.id)) }] : []),
    { label: s.hotkey ? "Change hotkey" : "Set hotkey", onSelect: () => setCapturing(s) },
    { label: "Delete", danger: true, onSelect: () => setDeleting(s) },
  ];

  if (state.sounds.length === 0) return <EmptyState dragOver={dragOver} />;

  const playingSet = new Set([...state.playing, ...optimistic]);
  const noDevices = !state.devices.speakers && !state.devices.virtual_mic;

  return (
    <div className={cx("min-h-full rounded-md border border-transparent", dragOver && "border-accent")}>
      {noDevices && <Banner />}
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
        <SortableContext items={state.sounds.map((s) => s.id)} strategy={rectSortingStrategy}>
          <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))" }}>
            {state.sounds.map((s) => (
              <Tile
                key={s.id}
                sound={s}
                playing={playingSet.has(s.id)}
                onPlay={() => void play(s.id)}
                onVolume={(v) => void run(api.setSoundVolume(s.id, v))}
                onContextMenu={(e) => { e.preventDefault(); setMenu({ sound: s, x: e.clientX, y: e.clientY }); }}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.sound)} onClose={() => setMenu(null)} />}
      {deleting && (
        <ConfirmDialog
          title={`Delete "${deleting.name}"? The file is removed from the soundboard.`}
          confirmLabel="Delete"
          danger
          onCancel={() => setDeleting(null)}
          onConfirm={() => { void run(api.deleteSound(deleting.id)); setDeleting(null); }}
        />
      )}
      {renaming && (
        <RenameDialog
          title="Name"
          initial={renaming.name}
          onCancel={() => setRenaming(null)}
          onSubmit={(name) => { void run(api.renameSound(renaming.id, name)); setRenaming(null); }}
        />
      )}
      {capturing && (
        <HotkeyCapture
          title={`Hotkey for "${capturing.name}"`}
          current={capturing.hotkey}
          onCancel={() => setCapturing(null)}
          onCapture={(accel) => { void run(api.setHotkey(capturing.id, accel)); setCapturing(null); }}
          onClear={capturing.hotkey ? () => { void run(api.clearHotkey(capturing.id)); setCapturing(null); } : undefined}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 6: Mount in App.tsx**

In `src/App.tsx` add `import { TileGrid } from "./components/TileGrid";` and replace the empty `<main ... />` with:
```tsx
<main className="min-w-0 flex-1 overflow-y-auto p-3">
  <TileGrid />
</main>
```

- [ ] **Step 7: Run tests, typecheck, dev-run**

Run: `npm test`
Expected: all pass (accel 6, store 4, Tile 5, ContextMenu 2, HotkeyCapture 3).

Run: `npm run build`
Expected: no type errors.

Run in background: `npm run tauri dev` and check:
- Empty state line appears with a working Add link. Dropping an mp3 from Explorer onto the window imports it.
- Tile shows its name; click plays (amber ring while playing); hover reveals the volume slider.
- Right-click menu: Rename works, Set image accepts a png and the tile shows it edge to edge, Set hotkey captures Ctrl+Shift+1 and the badge shows `Ctrl+Shift+1`; pressing it with another window focused plays the sound. A second sound bound to the same combo gets the toast "Ctrl+Shift+Digit1 is already used by <name>".
- Drag a tile a few pixels: it reorders; order survives restart.
- Delete asks for confirmation; Esc stops all.
Stop it.

- [ ] **Step 8: Commit**

```bash
git add src
git commit -m "feat(ui): tile grid with images, reorder, context menu, hotkey capture and drag-drop import"
```

---

### Task 15: Settings and Remote side panels

**Files:**
- Create: `src/lib/validate.ts`, `src/lib/validate.test.ts`, `src/components/SettingsPanel.tsx`, `src/components/SettingsPanel.test.tsx`, `src/components/RemotePanel.tsx`, `src/components/RemotePanel.test.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `api`, `useStore`, `HotkeyCapture`, `displayAccel`.
- Produces: `validatePort(value: string): string | null`, `validatePin(value: string): string | null` (null means valid, otherwise the message to show), `<SettingsPanel />`, `<RemotePanel />`.

- [ ] **Step 1: Write failing tests**

`src/lib/validate.test.ts`:
```ts
import { validatePin, validatePort } from "./validate";

describe("validatePort", () => {
  it("accepts 1024..65535", () => {
    expect(validatePort("1024")).toBeNull();
    expect(validatePort("65535")).toBeNull();
    expect(validatePort("7331")).toBeNull();
  });
  it("rejects everything else", () => {
    expect(validatePort("80")).toMatch(/1024/);
    expect(validatePort("65536")).toMatch(/65535/);
    expect(validatePort("abc")).toMatch(/number/);
    expect(validatePort("")).toMatch(/number/);
  });
});

describe("validatePin", () => {
  it("accepts 4 to 8 digits", () => {
    expect(validatePin("1234")).toBeNull();
    expect(validatePin("12345678")).toBeNull();
  });
  it("rejects wrong length or non digits", () => {
    expect(validatePin("123")).toMatch(/4 to 8 digits/);
    expect(validatePin("123456789")).toMatch(/4 to 8 digits/);
    expect(validatePin("12a4")).toMatch(/4 to 8 digits/);
  });
});
```

`src/components/SettingsPanel.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { useStore } from "../lib/store";
import { SettingsPanel } from "./SettingsPanel";

vi.mock("../lib/api", () => ({ api: {} }));

const base = {
  master_volume: 0.8,
  devices: { speakers: "Ghost Speakers", virtual_mic: null, available: [{ name: "Speakers (Realtek)", is_default: true }, { name: "CABLE Input", is_default: false }] },
  stop_all_hotkey: "Ctrl+Shift+F12",
  remote: { port: 7331, error: null, pin_set: false, addresses: ["192.168.1.20"], clients: 0 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

describe("SettingsPanel", () => {
  beforeEach(() => useStore.setState({ state: base }));

  it("lists devices and keeps a missing configured device selectable", () => {
    render(<SettingsPanel />);
    const speakers = screen.getByLabelText("Speakers") as HTMLSelectElement;
    expect(speakers.value).toBe("Ghost Speakers");
    expect(screen.getByText("Ghost Speakers (missing)")).toBeInTheDocument();
    expect(screen.getByText("Speakers (Realtek) (default)")).toBeInTheDocument();
    expect((screen.getByLabelText("Virtual mic") as HTMLSelectElement).value).toBe("");
  });

  it("shows the Stop All hotkey and the PIN set form", () => {
    render(<SettingsPanel />);
    expect(screen.getByText("Ctrl+Shift+F12")).toBeInTheDocument();
    expect(screen.getByLabelText("PIN")).toBeInTheDocument();
    expect(screen.getByText(/local network only/i)).toBeInTheDocument();
  });

  it("offers Remove when a PIN is set", () => {
    useStore.setState({ state: { ...base, remote: { ...base.remote, pin_set: true } } });
    render(<SettingsPanel />);
    expect(screen.getByText("Remove")).toBeInTheDocument();
  });
});
```

`src/components/RemotePanel.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { useStore } from "../lib/store";
import { RemotePanel } from "./RemotePanel";

vi.mock("../lib/api", () => ({ api: {} }));

const base = {
  master_volume: 0.8,
  devices: { speakers: null, virtual_mic: null, available: [] },
  stop_all_hotkey: "Ctrl+Shift+F12",
  remote: { port: 7331, error: null, pin_set: false, addresses: ["192.168.1.20", "10.0.0.5"], clients: 2 },
  sounds: [],
  playing: [],
  startup_notices: [],
};

describe("RemotePanel", () => {
  it("shows every URL, a QR code and the client count", () => {
    useStore.setState({ state: base });
    render(<RemotePanel />);
    expect(screen.getByText("http://192.168.1.20:7331")).toBeInTheDocument();
    expect(screen.getByText("http://10.0.0.5:7331")).toBeInTheDocument();
    expect(document.querySelector("svg")).toBeInTheDocument();
    expect(screen.getByText(/2 phones connected/)).toBeInTheDocument();
  });

  it("shows the error and Retry when the server is down", () => {
    useStore.setState({ state: { ...base, remote: { ...base.remote, port: null, error: "Could not open port 7331 or the next ten: in use" } } });
    render(<RemotePanel />);
    expect(screen.getByText(/Server not running/)).toBeInTheDocument();
    expect(screen.getByText(/Could not open port/)).toBeInTheDocument();
    expect(screen.getByText("Retry")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test`
Expected: new test files fail with module not found.

- [ ] **Step 3: validate.ts**

```ts
export function validatePort(value: string): string | null {
  if (!/^\d+$/.test(value)) return "Port must be a number";
  const n = Number(value);
  if (n < 1024 || n > 65535) return "Port must be between 1024 and 65535";
  return null;
}

export function validatePin(value: string): string | null {
  return /^\d{4,8}$/.test(value) ? null : "PIN must be 4 to 8 digits";
}
```

- [ ] **Step 4: SettingsPanel.tsx**

```tsx
import { useState } from "react";
import { displayAccel } from "../lib/accel";
import { api, type DeviceInfo } from "../lib/api";
import { useStore } from "../lib/store";
import { validatePin, validatePort } from "../lib/validate";
import { HotkeyCapture } from "./HotkeyCapture";

function DeviceSelect({ id, label, value, available, onChange, onTest }: {
  id: string; label: string; value: string | null; available: DeviceInfo[];
  onChange: (v: string | null) => void; onTest: () => void;
}) {
  const missing = value && !available.some((d) => d.name === value);
  return (
    <div className="mb-4">
      <label className="label" htmlFor={id}>{label}</label>
      <div className="flex gap-2">
        <select id={id} className="field" value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
          <option value="">None</option>
          {missing && <option value={value}>{value} (missing)</option>}
          {available.map((d) => (
            <option key={d.name} value={d.name}>{d.name}{d.is_default ? " (default)" : ""}</option>
          ))}
        </select>
        <button type="button" className="btn" disabled={!value} onClick={onTest}>Test</button>
      </div>
    </div>
  );
}

export function SettingsPanel() {
  const state = useStore((s) => s.state)!;
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [capturing, setCapturing] = useState(false);
  const [port, setPort] = useState(String(state.remote.port ?? 7331));
  const [pin, setPin] = useState("");
  const { speakers, virtual_mic, available } = state.devices;

  const applyPort = () => {
    const err = validatePort(port);
    if (err) { toast("error", err); return; }
    void run(api.setRemotePort(Number(port)));
  };

  const applyPin = () => {
    const err = validatePin(pin);
    if (err) { toast("error", err); return; }
    void run(api.setPin(pin));
    setPin("");
  };

  return (
    <aside className="panel">
      <DeviceSelect id="dev-speakers" label="Speakers" value={speakers} available={available}
        onChange={(v) => void run(api.setDevices(v, virtual_mic))} onTest={() => void run(api.testTone(speakers))} />
      <DeviceSelect id="dev-mic" label="Virtual mic" value={virtual_mic} available={available}
        onChange={(v) => void run(api.setDevices(speakers, v))} onTest={() => void run(api.testTone(virtual_mic))} />

      <div className="mb-4">
        <span className="label">Stop All hotkey</span>
        <div className="flex items-center gap-2">
          <span className="font-mono">{displayAccel(state.stop_all_hotkey)}</span>
          <button type="button" className="btn ml-auto" onClick={() => setCapturing(true)}>Change</button>
        </div>
      </div>

      <div className="mb-4">
        <label className="label" htmlFor="port">Remote port</label>
        <div className="flex gap-2">
          <input id="port" className="field" inputMode="numeric" value={port} onChange={(e) => setPort(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") applyPort(); }} />
          <button type="button" className="btn" onClick={applyPort} disabled={Number(port) === state.remote.port}>Apply</button>
        </div>
      </div>

      <div className="mb-2">
        <label className="label" htmlFor="pin">PIN</label>
        {state.remote.pin_set ? (
          <div className="flex items-center gap-2">
            <span className="text-dim">A PIN is set</span>
            <button type="button" className="btn ml-auto border-danger text-danger" onClick={() => void run(api.setPin(null))}>Remove</button>
          </div>
        ) : (
          <div className="flex gap-2">
            <input id="pin" className="field" inputMode="numeric" placeholder="4 to 8 digits" value={pin}
              onChange={(e) => setPin(e.target.value.replace(/\D/g, "").slice(0, 8))}
              onKeyDown={(e) => { if (e.key === "Enter") applyPin(); }} />
            <button type="button" className="btn" onClick={applyPin} disabled={!pin}>Set</button>
          </div>
        )}
      </div>
      <p className="text-xs text-mute">
        The remote is for your local network only. Traffic is unencrypted. The PIN stops other people on your
        Wi-Fi from triggering sounds; it is not protection against a hostile network.
      </p>

      {capturing && (
        <HotkeyCapture
          title="Stop All hotkey"
          current={state.stop_all_hotkey}
          onCancel={() => setCapturing(false)}
          onCapture={(accel) => { void run(api.setStopAllHotkey(accel)); setCapturing(false); }}
        />
      )}
    </aside>
  );
}
```

- [ ] **Step 5: RemotePanel.tsx**

```tsx
import { QRCodeSVG } from "qrcode.react";
import { api } from "../lib/api";
import { useStore } from "../lib/store";

export function RemotePanel() {
  const remote = useStore((s) => s.state!.remote);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);

  if (remote.port === null) {
    return (
      <aside className="panel">
        <p className="mb-1">Server not running.</p>
        {remote.error && <p className="mb-3 text-xs text-dim">{remote.error}</p>}
        <button type="button" className="btn" onClick={() => void run(api.restartRemote())}>Retry</button>
      </aside>
    );
  }

  const urls = remote.addresses.map((ip) => `http://${ip}:${remote.port}`);
  const copy = (u: string) => {
    void navigator.clipboard.writeText(u).then(() => toast("info", "Copied"));
  };

  return (
    <aside className="panel">
      {urls.length === 0 && <p className="mb-3 text-dim">No network connection.</p>}
      {urls[0] && (
        <div className="mb-4 inline-block rounded-md bg-surface p-3">
          <QRCodeSVG value={urls[0]} size={160} bgColor="#141416" fgColor="#ededef" level="M" />
        </div>
      )}
      <ul className="mb-4 space-y-1">
        {urls.map((u) => (
          <li key={u} className="flex items-center gap-2">
            <span className="select-text font-mono text-xs">{u}</span>
            <button type="button" className="btn ml-auto h-7 px-2 text-xs" onClick={() => copy(u)}>Copy</button>
          </li>
        ))}
      </ul>
      <p className="mb-4 text-dim">
        {remote.clients === 0 ? "No phones connected" : remote.clients === 1 ? "1 phone connected" : `${remote.clients} phones connected`}
      </p>
      <p className="text-xs text-mute">
        If your phone cannot connect, allow Soundboard through Windows Firewall on private networks.
      </p>
    </aside>
  );
}
```

- [ ] **Step 6: Mount panels in App.tsx**

Add imports for `SettingsPanel` and `RemotePanel` plus `const panel = useStore((s) => s.panel);`, and inside the flex row after `<main>`:
```tsx
{panel === "settings" && <SettingsPanel />}
{panel === "remote" && <RemotePanel />}
```

- [ ] **Step 7: Run tests, typecheck, dev-run**

Run: `npm test`
Expected: all pass (validate 4, SettingsPanel 3, RemotePanel 2, plus earlier 20).

Run: `npm run build`
Expected: no type errors.

Run in background: `npm run tauri dev` and check:
- Settings panel pushes the grid, lists real WASAPI devices, Test plays a short beep on the chosen device, choosing a VB-Cable device plus speakers makes sounds play on both.
- Change the Stop All hotkey; press it from another window while a sound plays.
- Set port 8000 then Apply: Remote panel URL changes to :8000.
- Set PIN 1234: on the phone (same Wi-Fi) the page asks for the PIN; wrong PIN five times gives the lockout message; right PIN connects; the Remote button shows an amber dot and the panel says "1 phone connected". Remove PIN: phone reconnects without asking.
Stop it.

- [ ] **Step 8: Commit**

```bash
git add src
git commit -m "feat(ui): settings and remote side panels"
```

---

### Task 16: Design pass, release build, README with manual checklist

**Files:**
- Create: `README.md`
- Modify: any `src/**` file that fails the design checklist

- [ ] **Step 1: Design review against the spec principles**

Run `npm run tauri dev`, add three sounds (one with an image), open each panel, and check every line. Fix any failure in the component responsible and re-check.

- Only two colors appear besides images: amber (playing ring, focus ring, remote dot, OK key on phone) and red (Stop, Delete, Remove, error toasts). No other hue anywhere.
- No icons except the speaker glyph beside the master slider.
- Tiles: 120 px minimum, 8 px gap, square, name bottom-left, badge top-right monospace, slider only on hover or focus.
- Panels push the grid; the grid stays clickable while a panel is open.
- Tab moves between tiles, Enter or Space plays, Esc stops all. Focus ring visible.
- Toasts bottom-left, at most three, gone after 4 s.
- Nothing in the window is a label with no action or information (no "Welcome", no section headers).
- Window at 520×360: top bar does not wrap, grid shows at least 3 columns.

- [ ] **Step 2: README**

`README.md`:
````markdown
# Soundboard

Windows desktop soundboard. Plays local clips to your speakers and a virtual
microphone at the same time, from tiles, global hotkeys, or a phone on your Wi-Fi.

## Develop

```bash
npm install
npm run tauri dev
```

Tests:

```bash
cargo test -p soundboard-core
npm test
```

Phone page without the desktop app (fake sounds, optional PIN):

```bash
cargo run -p soundboard-core --example remote_dev 1234
```

## Build an installer

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/nsis/Soundboard_0.1.0_x64-setup.exe`.

## Virtual microphone

Install [VB-Cable](https://vb-audio.com/Cable/). In Soundboard Settings pick
"CABLE Input" as the Virtual mic and your real speakers as Speakers. In Discord
or your game, choose "CABLE Output" as the microphone. To keep your own voice,
use VB-Audio's VoiceMeeter or Windows "Listen to this device" on your mic routed
to CABLE Input.

## Data

Settings and copied files live in `%APPDATA%\co.carbonweb.soundboard\`.

## Manual checklist

- [ ] Sound plays on speakers alone, virtual mic alone, both.
- [ ] Discord test call hears the sound through the virtual mic.
- [ ] Hotkey fires while a game window is focused.
- [ ] Phone connects via QR, plays, sees playing highlight, Stop All works.
- [ ] PIN required after setting; old phone token rejected after PIN change.
- [ ] Drag reorder persists after restart.
- [ ] Unplugging the configured device: next play falls back to default with one toast.
- [ ] Corrupt `config.json` is backed up to `config.json.bak` and the app starts fresh with a toast.
````

- [ ] **Step 3: Release build**

Run in background (several minutes):
```bash
npm run tauri build
```
Expected: `src-tauri/target/release/bundle/nsis/Soundboard_0.1.0_x64-setup.exe` exists. Install it, launch, confirm the window opens and plays a sound.

- [ ] **Step 4: Walk the manual checklist**

Go through every README checklist item with real hardware. Items that need a second person or device (Discord call, phone) are done by the user; record which items were verified and which remain in the final report.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "docs: README with setup, VB-Cable notes and manual checklist"
```

---

## Self-review notes

- Spec coverage: config (T2), audio engine and cache (T3 to T6), library and images (T7), hotkeys (T8, T12, T14, T15), remote server, PIN, lockout, phone page (T9 to T11), Tauri wiring and events (T12), desktop UI with tiles, reorder, context menu, panels, toasts, banner, empty state (T13 to T15), error table behaviors (T12 commands and state), testing strategy (each task), manual checklist (T16).
- One addition beyond the spec: `GET /auth/status` so the phone page can tell "PIN required" from "server unreachable" without burning a failed-PIN attempt. The spec's routes table is updated to include it.
- Names used consistently: `Engine::play_file`, `App::play`, `emit_state`, `Hub::broadcast_state`, `RemoteHandler`, `UiState`, `api.*`, `useStore`, `accelFromEvent`, `displayAccel`, `validatePort`, `validatePin`.
