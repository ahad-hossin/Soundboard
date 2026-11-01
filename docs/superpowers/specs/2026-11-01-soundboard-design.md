# Soundboard — Design Spec

Date: 2026-11-01
Status: Approved by user, awaiting implementation plan

## Goal

A Windows desktop soundboard. The user adds local audio files, triggers them by
clicking tiles, pressing global hotkeys, or tapping buttons on a phone web page
served over the local network. Audio plays simultaneously to speakers and to a
virtual microphone device (for example VB-Cable) so the user hears what Discord
or game voice chat hears.

## Scope

### In v1

- Windows desktop app built with Tauri 2 + React + Tailwind.
- Sounds from local files only: mp3, wav, ogg, flac.
- Dual output: play to up to two WASAPI output devices at once.
- Per-sound volume and master volume.
- Stop All.
- Global hotkeys per sound plus one for Stop All.
- Phone remote: web page on LAN with the same sound grid, master volume, Stop All.
- Optional PIN lock on the phone remote.

### Deferred (not v1)

- Pages / folders / tabs of sounds.
- "Cut previous sound" overlap mode (v1 always overlaps).
- YouTube/URL download, mic recording, clip trimming.
- MIDI / Stream Deck integration.
- HTTPS on the remote server.
- macOS / Linux builds.

## Architecture

```
Tauri app (single process)
├─ Rust core
│  ├─ audio engine    rodio/cpal, two output devices, per-sound + master gain
│  ├─ library store   config.json + sounds/ folder in app data dir
│  ├─ hotkeys         tauri-plugin-global-shortcut, key -> sound id
│  └─ remote server   axum on 0.0.0.0:<port>, serves phone page + WebSocket
├─ Desktop UI         React, calls Rust via Tauri commands, listens to events
└─ Phone page         single bundled HTML/JS file, served by axum
```

### Principle: one play path

Every trigger (tile click, hotkey, phone tap) ends in the same Rust function
`play(sound_id)`. Rust owns all state. After any state change Rust emits a
`state-changed` event; the desktop UI receives it through Tauri events and phone
clients receive it through their WebSocket. Neither frontend holds logic beyond
rendering and sending commands.

### Rust modules

| Module | Responsibility | Depends on |
|---|---|---|
| `config` | Load/save `config.json`; typed `Config` struct; backup on corrupt | serde, fs |
| `library` | Add/rename/delete sounds; copy files into `sounds/`; validate decodability | config, audio (decode check) |
| `audio` | `AudioBackend` trait; `RodioBackend` impl; device enumeration; play/stop; live gain | rodio, cpal |
| `hotkeys` | Register/unregister shortcuts; conflict detection; map key -> action | tauri-plugin-global-shortcut, config |
| `remote` | axum router; static page; WebSocket hub; PIN auth; lockout | axum, tokio, config |
| `state` | Shared `AppState` (`Arc<Mutex<...>>`); emits `state-changed` | all above |
| `commands` | Tauri `#[command]` functions the React UI calls | state |

## Data model

### `config.json`

```json
{
  "version": 1,
  "master_volume": 0.8,
  "devices": {
    "speakers": "Speakers (Realtek High Definition Audio)",
    "virtual_mic": "CABLE Input (VB-Audio Virtual Cable)"
  },
  "remote": {
    "port": 7331,
    "pin_hash": null,
    "token_secret": "random-32-bytes-base64"
  },
  "stop_all_hotkey": "Ctrl+Shift+Escape",
  "sounds": [
    {
      "id": "uuid",
      "name": "Airhorn",
      "file": "airhorn.mp3",
      "volume": 1.0,
      "hotkey": "Ctrl+Shift+1",
      "color": "#ff6b6b"
    }
  ]
}
```

- `devices.*` may be `null` meaning "not used". At least one should be set for
  sound to be heard; the UI warns if both are null.
- Device identity is by name string. cpal on Windows exposes stable friendly
  names; IDs are not stable across reboots.
- `file` is relative to `<app_data>/sounds/`. Adding a sound copies the source
  file in and renames on collision (`airhorn (2).mp3`).
- `pin_hash` is argon2 of the PIN, or null for no lock.
- `token_secret` signs remote auth tokens; regenerated whenever the PIN changes,
  which invalidates all existing phone tokens.

### Storage location

`%APPDATA%\soundboard\` (via Tauri `app_data_dir`):

```
config.json
config.json.bak      (written only when config.json failed to parse)
sounds/
  airhorn.mp3
  ...
```

## Audio engine

### Devices

- On startup and on Settings open, enumerate output devices via `cpal`
  (WASAPI host). Settings shows two dropdowns, "Speakers" and "Virtual mic",
  each with a "None" option and a "Test tone" button.
- If a configured device name is not found at launch, that slot falls back to
  the system default output for the session and a toast says which device was
  missing. Config is not modified.

### Playback

- `play(sound_id)`:
  1. Fetch decoded PCM buffer from cache, or decode the file with
     `rodio::Decoder` into a `rodio::buffer::SamplesBuffer` and insert into cache.
  2. For each enabled output device, create a `rodio::Sink` on that device's
     `OutputStream`, set volume to `sound.volume * master_volume`, append a
     clone of the buffer.
  3. Record the active sinks under a `play_instance_id` in `AppState`.
  4. Emit `state-changed` with `playing: [sound_id, ...]`.
- Sounds overlap: each play creates fresh sinks. Finished sinks are reaped by a
  background task that polls `Sink::empty()` every 100 ms and emits
  `state-changed` when the playing set changes.
- `stop_all()` drops every active sink and emits `state-changed`.
- Changing a sound's volume or master volume updates `Sink::set_volume` on all
  matching active sinks immediately.
- `OutputStream` per device is opened once and kept for the app lifetime; it is
  reopened if the device changes in Settings.

### Decoded buffer cache

- LRU keyed by sound id, evicting when total bytes exceed 200 MB.
- Invalidated for a sound when its file is replaced or deleted.

### Testability

- `trait AudioBackend { fn devices(); fn play(device, buffer, gain) -> SinkHandle; fn stop(handle); fn set_gain(handle, gain); }`
- `RodioBackend` is the real one. `FakeBackend` records calls for unit tests.
  Real audio output is verified with a manual checklist, not automated tests.

## Hotkeys

- Uses `tauri-plugin-global-shortcut`. Shortcuts fire while any other app is
  focused.
- Each sound has an optional `hotkey` string in the plugin's accelerator format
  (for example `Ctrl+Shift+1`). One global `stop_all_hotkey`, default
  `Ctrl+Shift+Escape`.
- Binding flow: user clicks "Set hotkey" on a tile, the UI captures the next
  key combination, sends it to Rust. Rust checks for conflicts against all other
  sounds and the Stop All key. On conflict it returns an error naming the owner
  and does not register. On success it registers with the plugin and saves.
- "Clear hotkey" unregisters and saves.
- All hotkeys are registered on startup from config. A shortcut the OS refuses
  (already taken system-wide) is reported with a toast and left unbound in
  memory while remaining in config so the user can see and change it.

## Phone remote

### Server

- `axum` server on a tokio task, bound to `0.0.0.0:<port>`, default 7331.
- If the port is busy, try the next 10 ports in order, then give up with an
  error toast. The actual port used is shown in the Remote panel.
- First launch triggers the Windows Firewall prompt. The Remote panel shows a
  hint: "If your phone cannot connect, allow Soundboard through Windows
  Firewall on private networks."
- Remote panel in the desktop UI shows the full URL for each non-loopback IPv4
  address (usually one) and a QR code of the first.

### Routes

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/` | none | Phone page HTML |
| GET | `/manifest.json`, `/icon.png` | none | PWA install |
| POST | `/auth` | none | Body `{pin}`; returns `{token}` or 401 |
| GET | `/ws?token=...` | token (if PIN set) | WebSocket |

### WebSocket protocol

Server -> client on connect and on every change:

```json
{ "type": "state",
  "master_volume": 0.8,
  "playing": ["uuid"],
  "sounds": [{ "id": "uuid", "name": "Airhorn", "color": "#ff6b6b", "volume": 1.0 }] }
```

Client -> server:

```json
{ "type": "play", "id": "uuid" }
{ "type": "stop_all" }
{ "type": "set_master_volume", "value": 0.5 }
```

Unknown or malformed messages are ignored and logged.

### Phone page

- One HTML file with inline CSS and JS, no framework, embedded in the binary
  with `include_str!`.
- Layout: top bar with master volume slider and Stop All; below, a responsive
  grid of large tap targets showing sound name on the sound's color. A tile
  highlights while its id is in `playing`.
- If the server responds 401 on WebSocket connect, show a PIN entry screen.
  On success store the token in `localStorage` and reconnect.
- Auto-reconnect with backoff on socket close.
- Includes a web app manifest so it can be added to the home screen and run
  full-screen.

### PIN lock

- PIN is 4 to 8 digits, set in desktop Settings. Blank means no lock, and then
  `/ws` accepts connections without a token.
- `POST /auth` verifies against `pin_hash` (argon2). On success returns an
  HMAC-SHA256 token over a random nonce using `token_secret`.
- Lockout: 5 failed attempts from one IP within 10 minutes blocks that IP for
  60 seconds (429 response). Counter lives in memory.
- Changing or clearing the PIN regenerates `token_secret`, which invalidates
  every issued token. Connected phone sockets are closed and must re-auth.
- Security statement shown in Settings: the remote is for your local network
  only. Traffic is unencrypted. The PIN stops other people on your Wi-Fi from
  triggering sounds; it is not protection against a hostile network.

## Desktop UI

### Main view

- Grid of sound tiles, auto-fit columns, minimum tile width 140 px.
- Tile: name, hotkey badge (if set), a small volume slider, background in the
  sound's color. Click plays. Glows while playing.
- Right-click context menu: Rename, Set hotkey, Clear hotkey, Change color,
  Delete (with confirm).
- Empty state: "Drop audio files here or click Add sounds."

### Top bar

- Master volume slider, Stop All button, Add sounds button, Remote toggle
  (opens Remote panel), Settings gear.

### Add sounds

- Multi-file picker via Tauri dialog plugin, filtered to mp3/wav/ogg/flac.
- Drag-and-drop of files onto the window.
- Each file is test-decoded before being copied in. Undecodable files are
  rejected with a toast naming the file; the others proceed.
- Name defaults to the file name without extension. Color is picked from a
  fixed palette in rotation.

### Settings modal

- Speakers device dropdown + Test tone.
- Virtual mic device dropdown + Test tone.
- Remote port (number input), applies on save by restarting the server.
- PIN (set / clear).
- Stop All hotkey (capture control).
- Security statement text for the remote.

### Remote panel

- URL(s), QR code, connected client count, firewall hint.

## Error handling

| Situation | Behavior |
|---|---|
| Configured device missing at launch | Use default output for that slot this session; toast once |
| Device disappears mid-session | Next play falls back to default output; toast once per session |
| Both device slots null | Banner in main view: "No output device selected" |
| Undecodable file on add | Toast with file name; file not copied |
| Sound file missing on play | Toast; tile shows warning icon; sound stays in list |
| Port and next 10 busy | Toast; Remote panel shows "Server not running" with Retry |
| `config.json` unparsable | Copy to `config.json.bak`, start with defaults, toast |
| Hotkey refused by OS | Toast naming the key; left unbound this session |
| Remote wrong PIN | 401; after 5 in 10 min, 429 for 60 s |

## Testing

### Rust unit tests

- `config`: default round-trip, load of each schema version, corrupt file
  produces `.bak` and defaults.
- `hotkeys`: conflict detection across sounds and Stop All; accelerator string
  normalization.
- `remote::auth`: PIN hash verify, token issue/verify, token invalid after
  secret rotation, lockout counter timing (with injected clock).
- `library`: name collision renaming; rejection of undecodable input (using a
  tiny invalid file fixture and a tiny valid wav fixture).
- `audio` with `FakeBackend`: play fans out to each enabled device with gain
  = sound × master; stop_all stops every handle; volume change updates active
  handles; cache eviction at the byte limit.

### Rust integration tests

- axum router with `tower::ServiceExt::oneshot`: `/` returns HTML; `/auth`
  flows; `/ws` rejects without token when PIN set, accepts when no PIN.

### Frontend tests

- vitest + React Testing Library: tile grid renders sounds and playing state;
  settings form validates PIN length and port range; hotkey capture control
  formats accelerators.

### Manual checklist (real audio)

- Sound plays on speakers alone, virtual mic alone, both.
- Discord test call hears the sound through the virtual mic.
- Hotkey fires while a game window is focused.
- Phone connects via QR, plays, sees playing highlight, Stop All works.
- PIN required after setting; old phone token rejected after PIN change.

## Project layout

```
soundboard/
├─ src/                  React desktop UI
│  ├─ components/        TileGrid, Tile, TopBar, SettingsModal, RemotePanel, HotkeyCapture
│  ├─ lib/tauri.ts       typed wrappers around invoke/listen
│  └─ App.tsx
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ state.rs
│  │  ├─ commands.rs
│  │  ├─ config.rs
│  │  ├─ library.rs
│  │  ├─ audio/{mod.rs, backend.rs, rodio_backend.rs, fake_backend.rs, cache.rs}
│  │  ├─ hotkeys.rs
│  │  └─ remote/{mod.rs, routes.rs, ws.rs, auth.rs, page.html}
│  ├─ Cargo.toml
│  └─ tauri.conf.json
├─ docs/superpowers/specs/
└─ package.json
```
