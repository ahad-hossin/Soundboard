# Add from link, bass boost, volume past 100% — Design Spec

Date: 2026-11-16
Status: Approved by user, awaiting implementation

## Goal

Three small features:

1. Paste a link (YouTube, Instagram, Facebook, anything yt-dlp supports) and get a sound tile.
2. A per-sound bass boost with a 0–100% strength, non-destructive like trim.
3. Per-sound volume up to 200%, with clipping handled by a soft limiter.

## Scope

### In

- Desktop "From link" button and paste dialog; the sound lands in the open folder.
- Installer offers to install yt-dlp and ffmpeg with winget.
- One automatic `winget upgrade` of yt-dlp and retry when a download fails.
- Bass boost slider in the right-click menu; saved per sound.
- Per-sound volume 0–200% on desktop tiles and the phone volume sheet.

### Out (cut on purpose)

- Add from link on the phone page.
- Cancel button or progress bar for downloads.
- Master volume past 100% (stays 0–100%).
- Cookies / login for sites that need it (login-only Instagram posts fail with yt-dlp's message).
- Bundling or self-downloading yt-dlp/ffmpeg.
- Live bass changes on a sound that is already playing (applies on the next play).

## 1. Add from link

### Tools: installed by winget, found at run time

- **Installer:** an NSIS hook (`bundle.windows.nsis.installerHooks` → `src-tauri/installer-hooks.nsh`,
  macro `NSIS_HOOK_POSTINSTALL`) asks with a Yes/No message box:
  "Install yt-dlp and ffmpeg so Soundboard can add sounds from links? (uses winget)".
  Yes runs `winget install -e --id yt-dlp.yt-dlp` and `winget install -e --id Gyan.FFmpeg`
  (with `--accept-source-agreements --accept-package-agreements`), waiting for each.
  Silent installs skip the prompt. A missing winget or a failed install does not fail the
  app install. winget owns updates and uninstall of the tools.
- **Lookup (`core/src/link.rs`, `find_tool(name)`):** search the directories of the process
  PATH, then the user and machine PATH read fresh from the registry
  (`HKCU\Environment\Path`, `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment\Path`),
  then `%LOCALAPPDATA%\Microsoft\WinGet\Links`. The fresh read matters because winget edits
  PATH after the app process (started by the installer's "Run" checkbox) already copied it.
  The pure part (split a PATH string, expand `%VAR%`, pick the first dir containing
  `<name>.exe`) is unit tested.
- **Missing tool:** error "yt-dlp is not installed. Install it with: winget install -e --id yt-dlp.yt-dlp"
  (ffmpeg likewise with `Gyan.FFmpeg`).

### Download

`add_sound_from_url(url, folder) -> String` (new sound id), async Tauri command:

1. Validate: trimmed URL must start with `http://` or `https://`; else "That is not a link".
2. Find yt-dlp and ffmpeg.
3. In a fresh temp dir, run with no console window (`CREATE_NO_WINDOW`) and a 3-minute timeout:
   `yt-dlp --no-playlist -x --audio-format mp3 --ffmpeg-location <ffmpeg dir>
   --match-filter "duration<=?600" --no-progress -o "<tmp>/%(title).80B.%(ext)s" <url>`. Media with unknown duration (live streams, some
   Instagram/Facebook posts) passes the filter and is bounded only by the 3-minute timeout.
   Args are passed as a vector, never through a shell.
4. Failure (non-zero exit or no `.mp3` produced): if the tool came from winget, run
   `winget upgrade -e --id yt-dlp.yt-dlp` once (no window, 2-minute timeout) and retry step 3
   once. Still failing: return the last stderr line starting with `ERROR:` (prefix stripped),
   or "Download failed" if there is none. A video longer than 10 minutes yields no file and the
   match-filter message; we report "Videos longer than 10 minutes are not supported".
5. Success: `Library::import_sound` on the mp3 (copies into `sounds/`, name = file stem = video
   title), set `folder` like `add_sounds` (unknown folder falls back to top level), save, refresh,
   emit state. The temp dir is deleted in every case.

The 10-minute cap exists because sounds are fully decoded into memory (1 hour ≈ 1.3 GB).
The decode cache was raised from 200 MB to 400 MB for this reason: a 10-minute 48 kHz stereo
sound decodes to about 230 MB and must fit next to the other sounds.

### UI

- `TopBar`: "From link" button next to "Add sounds". Opens a small dialog (existing dialog
  pattern in `Dialogs.tsx`) with a URL input and Add / Cancel.
- Add closes the dialog and calls `addFromLink(url)` in `src/lib/actions.ts`; the button
  label shows "Downloading…" and is disabled while any link download runs (store flag).
- Success opens the trim editor for the new sound (same as a single-file import).
  Failure shows an error toast with the message above.

## 2. Bass boost

### Data

`Sound.bass: f32`, `#[serde(default)]` = 0.0 (off). `sanitize` clamps to 0..=1 (non-finite → 0).
`CONFIG_VERSION` stays 1. `UiSound.bass` exposed. Command `set_bass(id, value)` saves at once.

### DSP (`core/src/audio/bass.rs`, pure, unit tested)

`bass_boost(buffer, strength) -> DecodedBuffer`, strength in 0..=1:

- strength 0 returns the input unchanged (shared `Arc`, no copy).
- Per channel: RBJ low-shelf biquad, 120 Hz, slope 1, gain `18 dB × strength`.
- Then saturation `tanh(drive·x)/tanh(drive)` with `drive = 1 + 3·strength`.
- Then normalize so the peak is at most 1.0 (never amplifies a quieter result).

### Engine

- Order: trim, then bass. The cached slice map becomes keyed by `(Option<Trim>, bass)` so the
  processed slice is built once per setting and reused; `set_trim`/`set_bass` drop it when
  the setting changed.
- `play_file` and `preview` take the bass strength, so "Play selection" in the trim editor
  sounds like the tile.

### UI

Right-click menu item "Bass boost…" opens a popover anchored to the tile (`usePopoverPosition`)
with a 0–100% range slider (`useSliderDraft`) and a percentage readout. Changes save on release.
Tiles with bass > 0 need no extra marker.

## 3. Volume past 100%

- `MAX_SOUND_VOLUME = 2.0` in `config.rs`. Per-sound volume clamps to 0..=2 in `sanitize`,
  `Engine::play_buffer` / `set_sound_volume`, `App::set_sound_volume`, and the remote message
  handler. Master stays 0..=1 everywhere.
- Tile slider `max={2}`; phone volume sheet `max="200"`.
- **Clipping:** the rodio backend moves gain from `Sink::set_volume` into `SharedSource`
  (an `Arc<AtomicU32>` holding the f32 bits, updated by `SetGain`; sink volume stays 1.0).
  When gain ≤ 1 samples pass as `x·gain`, bit-identical to today. When gain > 1 the product
  goes through a soft knee: below 0.9 unchanged, above it `0.9 + 0.1·tanh((|x|−0.9)/0.1)`
  with the sign kept, so output never exceeds 1.0. `soft_limit(x)` is a pure unit-tested fn.
  Several sounds playing at once can still sum past 1.0, as today.

## Testing

- core: `bass_boost` (zero strength shares samples, boosts a 50 Hz sine more than a 5 kHz sine,
  peak ≤ 1, stereo channels filtered independently), `soft_limit`, sanitize of `bass` and
  volume 2.0 / 3.0 / NaN, old config without `bass` loads, engine caches the processed slice and
  re-cuts on bass change, remote clamp to 2.0, `find_tool` path logic, yt-dlp error-line parsing
  and argument vector.
- src-tauri: existing tests keep passing; `set_sound_volume` keeps 1.5.
- UI (vitest): tile slider max is 2, "Bass boost…" menu item opens the slider and calls
  `setBass`, link dialog validates and calls `addSoundFromUrl`, button shows "Downloading…".
- No test hits the network or runs yt-dlp.

## README

New sections: "Add a sound from a link" (winget tools, 10-minute cap, what errors mean),
"Bass boost", and the 200% volume note.
