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
- Optional image per sound (png, jpg, webp, gif) shown on its tile.
- Dual output: play to up to two WASAPI output devices at once.
- Per-sound volume and master volume.
- Stop All.
- Global hotkeys per sound plus one for Stop All.
- Drag to reorder tiles.
- Phone remote: web page on LAN with the same sound grid, master volume, Stop All.
- Optional PIN lock on the phone remote.

### Deferred (not v1)

- Pages / folders / tabs of sounds.
- "Cut previous sound" mode that stops *other* sounds when one plays (v1 overlaps different sounds).
- YouTube/URL download, mic recording, clip trimming.
- MIDI / Stream Deck integration.
- HTTPS on the remote server.
- Light theme.
- macOS / Linux builds.

## Design principles (UI)

These rules apply to both the desktop UI and the phone page.

1. **Monochrome base.** Backgrounds, surfaces, borders, text are shades of one
   near-black to off-white ramp. No gradients for decoration, no colored icons,
   no colored chrome.
2. **Color means something.** Exactly two colors exist:
   - **Accent (amber `#F5B324`)** means *live*: a sound currently playing, the
     focused element, a connected remote client, the primary action.
   - **Danger (red `#EF4444`)** means *stop or destroy*: Stop All, Delete.
   Sound images are the only other source of color, and they are the content.
   Sound tiles are the exception: each sound carries one color from a fixed
   ten-color retro palette (`core/src/palette.rs`), assigned least-used-first at
   import and changeable from the context menu. Image tiles show it as a 3 px
   bottom bar.

   *Tile decoration and play animation.* A tile without an image layers a soft
   top-left highlight and one translucent white motif (circle, ring, diagonal
   band or dots, 12 to 14 % opacity, CSS only) over its color, under the name
   scrim. The motif is fixed per sound: the sum of the id's character codes
   mod 4. Image tiles are not decorated. A playing tile keeps its accent ring
   and gains a 2 px white progress bar along its bottom edge that grows
   linearly over the clip's duration and restarts on every play, whether it
   came from a click, a hotkey or the phone (the backend's per-sound play
   counter drives the restart); when the duration is unknown the bar pulses
   instead. Pressing a tile scales it to 96 % for the length of the press.
   The phone page draws the same decoration, sweep and press feedback.
3. **Images are the content.** A tile with an image is that image, edge to
   edge, with the name in a quiet label over a bottom scrim. A tile without an
   image is a flat surface with the name. Nothing else competes.
   Sound tiles are the exception: each sound carries one color from a fixed
   ten-color retro palette (`core/src/palette.rs`), assigned least-used-first at
   import and changeable from the context menu. Image tiles show it as a 3 px
   bottom bar.
4. **Every element earns its place.** No welcome text, no feature blurbs, no
   icons that duplicate labels, no section headers over obvious groups. If an
   element has no action or no information, it is removed.
5. **Panels, not modals.** Settings and Remote open as a right-side panel that
   pushes the grid, so the grid stays usable. Only destructive confirmation
   (Delete) uses a small dialog.
6. **Keyboard-first.** Tab moves between tiles, Enter or Space plays, Esc
   stops all while the window is focused. Focus is visible as an accent ring.
   Hotkeys are shown on tiles in a monospace badge.
7. **Immediate feedback.** A tile shows playing state within one frame of the
   click (optimistic), then follows Rust state. Toasts are small, bottom-left,
   auto-dismiss in 4 s, never stacked more than three.
8. **Density.** Tiles are square, minimum 120 px, 8 px gap. Typography is a
   system sans stack at 13 px body, 12 px labels, 11 px monospace badges.
   Controls are 32 px tall.

### Tokens

```
--bg          #0A0A0B
--surface     #141416
--surface-2   #1C1C1F
--border      #26262A
--text        #EDEDEF
--text-dim    #8A8A93
--text-mute   #55555C
--accent      #F5B324
--accent-ink  #1A1200   (text on accent)
--danger      #EF4444
--radius      8px
```

## Architecture

```
Tauri app (single process)
├─ Rust core crate (no Tauri dependency, unit-testable)
│  ├─ audio engine    rodio/cpal, two output devices, per-sound + master gain
│  ├─ library         config.json + sounds/ + images/ in app data dir
│  ├─ hotkeys logic   accelerator normalization + conflict detection
│  └─ remote server   axum, serves phone page + WebSocket, PIN auth
├─ Tauri app crate    wires core to Tauri: commands, events, global-shortcut plugin
├─ Desktop UI         React, calls Rust via Tauri commands, listens to events
└─ Phone page         single bundled HTML/JS file, served by axum
```

### Principle: one play path

Every trigger (tile click, hotkey, phone tap) ends in the same Rust function
`play(sound_id)`. Rust owns all state. After any state change Rust emits a
`state-changed` event; the desktop UI receives it through Tauri events and phone
clients receive it through their WebSocket. Neither frontend holds logic beyond
rendering and sending commands.

### Crates

| Crate | Path | Responsibility |
|---|---|---|
| `soundboard-core` | `core/` | Everything testable without Tauri: config, audio engine + backends, library, hotkey logic, remote server |
| `soundboard` (app) | `src-tauri/` | Tauri setup, commands, event emission, global-shortcut registration, remote server lifecycle |

### Core modules

| Module | Responsibility |
|---|---|
| `config` | `Config` struct, defaults, load/save with atomic write, backup on corrupt |
| `library` | Import/delete sounds and images, collision renaming, decodability check |
| `audio::backend` | `AudioBackend` trait, `DecodedBuffer`, `DeviceTarget`, `SinkHandle` |
| `audio::fake_backend` | Recording fake for tests |
| `audio::rodio_backend` | Real backend; owns `OutputStream`s on a dedicated audio thread |
| `audio::decode` | File to `DecodedBuffer` via rodio decoders |
| `audio::cache` | LRU of decoded buffers by sound id with byte cap |
| `audio::engine` | Fan-out to outputs, gains, active sink tracking, reaping, fallback |
| `hotkeys` | Accelerator normalization, conflict detection |
| `remote::auth` | PIN hashing, token issue/verify, IP lockout |
| `remote::protocol` | WebSocket message types |
| `remote` | axum router, WebSocket hub, static page, `RemoteHandler` trait |

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
  "stop_all_hotkey": "Ctrl+Shift+F12",
  "sounds": [
    {
      "id": "uuid",
      "name": "Airhorn",
      "file": "airhorn.mp3",
      "image": "uuid.png",
      "volume": 1.0,
      "hotkey": "Ctrl+Shift+Digit1",
      "color": "#FF595E"
    }
  ]
}
```

- Order of `sounds` is display order. Reordering rewrites the array.
- `devices.*` may be `null` meaning "not used". At least one should be set for
  sound to be heard; the UI shows a banner if both are null.
- Device identity is by name string. cpal on Windows exposes stable friendly
  names; IDs are not stable across reboots.
- `file` is relative to `<app_data>/sounds/`. Adding a sound copies the source
  file in and renames on collision (`airhorn (2).mp3`).
- `image` is relative to `<app_data>/images/`, named `<sound id>.<ext>`, or
  null.
- Hotkeys use `KeyboardEvent.code` key names with Title-case modifiers in the
  fixed order `Ctrl`, `Alt`, `Shift`, `Super`: for example `Ctrl+Shift+Digit1`,
  `Alt+KeyQ`, `F9`. This is the format `tauri-plugin-global-shortcut` parses.
- `pin_hash` is argon2 of the PIN, or null for no lock.
- `token_secret` signs remote auth tokens; regenerated whenever the PIN changes,
  which invalidates all existing phone tokens.
- Default Stop All hotkey is `Ctrl+Shift+F12`. (`Ctrl+Shift+Escape` is owned by
  Windows Task Manager and cannot be registered.)

### Storage location

Tauri `app_data_dir()`, which on Windows is
`%APPDATA%\com.yokitori.soundboard\`:

```
config.json
config.json.bak      (written only when config.json failed to parse)
sounds/
  airhorn.mp3
images/
  <sound id>.png
```

## Audio engine

### Devices

- On startup and on Settings open, enumerate output devices via `cpal`
  (WASAPI host). Settings shows two dropdowns, "Speakers" and "Virtual mic",
  each with a "None" option and a "Test" button that plays a 0.4 s sine tone.
- If a configured device name is not found when a sound plays, that slot falls
  back to the system default output and a toast says which device was missing,
  once per session per device. Config is not modified.

### Playback

- `play(sound_id)`:
  1. Fetch decoded PCM buffer from cache, or decode the file with
     `rodio::Decoder` into f32 samples and insert into cache.
  2. For each enabled output device, ask the backend to play the buffer at
     gain `sound.volume * master_volume`.
  3. Record the returned sink handles under a play instance in the engine.
  4. Emit `state-changed` with `playing: [sound_id, ...]`.
- Different sounds overlap. Playing a sound that is already playing stops its
  current sinks first and restarts it. Finished sinks are reaped by a
  background thread that polls every 100 ms and emits `state-changed` when the
  playing set changes.
- `stop_all()` stops every active sink and emits `state-changed`.
- Changing a sound's volume or master volume updates gain on all matching
  active sinks immediately.
- `OutputStream`s are owned by one audio thread (cpal streams are not thread
  safe) and opened lazily per device, kept for the app lifetime, and dropped
  when devices change in Settings.

### Decoded buffer cache

- LRU keyed by sound id, evicting when total bytes exceed 200 MB.
- Invalidated for a sound when its file is deleted.

### Testability

- `trait AudioBackend` with `devices`, `play`, `stop`, `set_gain`,
  `is_finished`, `reset`.
- `RodioBackend` is the real one. `FakeBackend` records calls for unit tests.
  Real audio output is verified with a manual checklist, not automated tests.

## Hotkeys

- Uses `tauri-plugin-global-shortcut`. Shortcuts fire while any other app is
  focused.
- Each sound has an optional `hotkey` accelerator. One global
  `stop_all_hotkey`, default `Ctrl+Shift+F12`.
- Binding flow: user clicks "Set hotkey" on a tile, the UI captures the next
  key combination (requires at least one modifier unless it is an F-key),
  sends it to Rust. Rust normalizes, checks for conflicts against all other
  sounds and the Stop All key. On conflict it returns an error naming the owner
  and does not register. On success it re-registers all shortcuts and saves.
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
| GET | `/manifest.json`, `/icon.svg` | none | PWA install |
| GET | `/img/{id}` | none | Sound image bytes |
| GET | `/auth/status?token=` | none | `{pin_required, token_valid}` so the page can tell "needs PIN" from "server down" |
| POST | `/auth` | none | Body `{pin}`; returns `{token}` or 401/429 |
| GET | `/ws?token=...` | token (if PIN set) | WebSocket |

### WebSocket protocol

Server -> client on connect and on every change:

```json
{ "type": "state",
  "master_volume": 0.8,
  "playing": ["uuid"],
  "sounds": [{ "id": "uuid", "name": "Airhorn", "volume": 1.0, "has_image": true, "color": "#FF595E" }] }
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
  with `include_str!`. Same tokens and principles as the desktop UI.
- Layout: top bar with connection dot, master volume slider; grid of square
  tiles, 3 columns in portrait, image edge to edge with name scrim and a 3 px color bar, or
  a flat tile in the sound's color with name; sticky bottom Stop All in danger style, full width.
- A tile highlights with the accent ring while its id is in `playing`.
  Tapping vibrates 10 ms where supported.
- On load the page calls `GET /auth/status`; if a PIN is required and the
  stored token is not valid it shows a PIN entry screen with a numeric keypad,
  otherwise it opens the WebSocket. On success it stores the token in
  `localStorage` and connects.
- Auto-reconnect with backoff (0.5 s doubling to 8 s) on socket close.
- Includes a web app manifest so it can be added to the home screen and run
  full-screen. `touch-action: manipulation`, no text selection.

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

- Grid of square tiles, auto-fill columns, minimum tile width 120 px, 8 px gap.
- Tile: image edge to edge with a 3 px bottom bar in the sound's color (or,
  without an image, a flat tile in that color), name label bottom-left on a
  bottom scrim, hotkey badge top-right in monospace, thin volume slider
  revealed on hover along the bottom edge. Click plays. Accent ring while
  playing. Accent ring on keyboard focus.
- Drag a tile to reorder. Order persists.
- Right-click context menu: Rename, Set image, Remove image, Set hotkey,
  Clear hotkey, Change color (opens a popover of the ten palette swatches, current
  one ringed), Delete. Delete is in danger color and opens a small confirm
  dialog.
- Empty state: a single line in `--text-dim`, centered: "Drop audio files
  here, or Add." where Add is a button.
- Banner above the grid when both device slots are null: "No output device
  selected. Open Settings." with Settings as a link.

### Top bar

Left: "Soundboard" in `--text-dim`, then sound count. Right, in order:
master volume (speaker glyph + 120 px range), Stop All (danger outline,
solid danger while anything plays), Add (neutral filled), Remote (toggle,
small accent dot when a phone is connected), Settings (toggle).

### Add sounds

- Multi-file picker via Tauri dialog plugin, filtered to mp3/wav/ogg/flac.
- Drag-and-drop of files onto the window. While dragging, the grid border
  turns accent.
- Each file is test-decoded before being copied in. Undecodable files are
  rejected with a toast naming the file; the others proceed.
- Name defaults to the file name without extension.

### Set image

- Context menu "Set image" opens a picker filtered to png/jpg/jpeg/webp/gif.
  The file is copied to `images/<sound id>.<ext>`. Validation uses file
  magic bytes, not extension.

### Side panel (Settings)

- Speakers device dropdown + Test.
- Virtual mic device dropdown + Test.
- Stop All hotkey (capture control).
- Remote port (number input 1024 to 65535), applies on save by restarting the
  server.
- PIN (set / clear), with the security statement underneath in `--text-mute`.

### Side panel (Remote)

- URL(s), QR code (monochrome, on `--surface`), connected client count,
  firewall hint in `--text-mute`.

## Error handling

| Situation | Behavior |
|---|---|
| Configured device missing on play | Use default output for that slot; toast once per session per device |
| Both device slots null | Banner in main view |
| Unsupported extension or undecodable file on add | Toast with file name; file not copied |
| Not an image on Set image | Toast; nothing changed |
| Sound file missing on play | Toast; tile shows a small warning glyph; sound stays in list |
| Port and next 10 busy | Toast; Remote panel shows "Server not running" with Retry |
| `config.json` unparsable | Copy to `config.json.bak`, start with defaults, toast |
| Hotkey refused by OS | Toast naming the key; left unbound this session |
| Remote wrong PIN | 401; after 5 in 10 min, 429 for 60 s |

## Testing

### Core crate unit tests (`cargo test -p soundboard-core`)

- `config`: default round-trip, missing file gives defaults, corrupt file
  produces `.bak` and defaults, atomic save.
- `hotkeys`: normalization of modifier aliases and order; conflict detection
  across sounds and Stop All; invalid accelerators rejected.
- `remote::auth`: PIN format check, hash verify, token issue/verify, token
  invalid after secret rotation, lockout counter with injected `Instant`.
- `library`: unsupported extension rejected; undecodable rejected; valid wav
  imported with collision renaming; image import validates magic bytes and
  replaces the old image; delete removes both files.
- `audio::cache`: LRU order, eviction at byte cap, remove.
- `audio::decode`: generated wav decodes to the expected sample count.
- `audio::engine` with `FakeBackend`: play fans out to each enabled device
  with gain = sound × master; missing named device falls back to default and
  reports it; no outputs is an error; stop_all stops every handle; volume
  change updates active handles; reap drops finished handles.

### Core crate integration tests

- axum router with `tower::ServiceExt::oneshot` and a fake `RemoteHandler`:
  `/` returns HTML; `/auth` wrong PIN 401, right PIN 200 with a verifying
  token, 6th wrong attempt 429; `/ws` without token when PIN set returns 401;
  `/img/{id}` 404 for unknown.

### Frontend tests (`npm test`)

- vitest + React Testing Library: accelerator capture and display helpers;
  Tile renders name, badge, playing ring; Settings panel validates PIN length
  and port range.

### Manual checklist (real audio and devices)

- Sound plays on speakers alone, virtual mic alone, both.
- Discord test call hears the sound through the virtual mic.
- Hotkey fires while a game window is focused.
- Phone connects via QR, plays, sees playing highlight, Stop All works.
- PIN required after setting; old phone token rejected after PIN change.
- Drag reorder persists after restart.

## Project layout

```
Soundboard/
├─ Cargo.toml                 workspace: core, src-tauri
├─ package.json
├─ core/                      soundboard-core crate
│  ├─ Cargo.toml
│  ├─ src/
│  │  ├─ lib.rs
│  │  ├─ config.rs
│  │  ├─ library.rs
│  │  ├─ hotkeys.rs
│  │  ├─ audio/{mod.rs, backend.rs, fake_backend.rs, rodio_backend.rs, decode.rs, cache.rs, engine.rs}
│  │  └─ remote/{mod.rs, auth.rs, protocol.rs, page.html, manifest.json, icon.svg}
│  └─ tests/remote_routes.rs
├─ src-tauri/                 Tauri app crate
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json
│  └─ src/{main.rs, lib.rs, state.rs, commands.rs, hotkeys_glue.rs, remote_glue.rs}
├─ src/                       React desktop UI
│  ├─ main.tsx, App.tsx, index.css
│  ├─ lib/{api.ts, accel.ts, store.ts}
│  └─ components/{TopBar, TileGrid, Tile, ContextMenu, ConfirmDialog, Toasts, HotkeyCapture, SettingsPanel, RemotePanel}.tsx
└─ docs/superpowers/{specs,plans}/
```
