# Add from link, bass boost, volume past 100% — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Paste a link to get a sound tile (yt-dlp + ffmpeg via winget), a per-sound bass boost (0–100%), and per-sound volume up to 200% with a soft limiter.

**Architecture:** Rust core (`core/`) holds the pure logic: `audio/bass.rs` (DSP), a soft limiter in the rodio backend, `link.rs` (tool lookup + yt-dlp runner). The Tauri shell (`src-tauri/`) adds two commands (`set_bass`, `add_sound_from_url`). The React UI adds a bass popover, a "From link" button and raises slider limits. An NSIS hook offers winget installs.

**Tech Stack:** Rust 2021, rodio 0.20, windows 0.62, Tauri 2, React 19 + zustand + Tailwind 4, vitest.

Spec: `docs/superpowers/specs/2026-11-16-link-bass-overdrive-design.md`.

## Global Constraints

- Branch `feat/link-bass-overdrive`; never commit to `master`.
- Every commit message ends with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- New config fields use `#[serde(default)]`; `CONFIG_VERSION` stays 1; configs without the new fields must load.
- Every config change calls `App::mark_dirty()` (saves at once).
- UI is monochrome; amber (`accent`) means playing only. Reuse `btn`, `field`, `label` classes.
- `MAX_SOUND_VOLUME = 2.0`; master volume stays 0..=1.
- Bass strength is 0..=1, default 0.0 (off).
- Link videos longer than 600 s are refused; yt-dlp runs with a 3-minute timeout; winget upgrade with a 2-minute timeout.
- Child processes run with `CREATE_NO_WINDOW` (0x08000000) and args as a vector (never a shell).
- Commands to verify: `cargo test -p soundboard-core`, `cargo test -p soundboard`, `npm test`, `npm run build`.
- Never kill the running installed app (`H:\Software\Soundboard`).

---

### Task 1: Per-sound volume up to 200% (clamps and sliders)

**Files:**
- Modify: `core/src/config.rs` (constant, `unit` → `clamp_or`, sanitize, tests)
- Modify: `core/src/audio/engine.rs` (`set_sound_volume`, `play_buffer` clamps, test)
- Modify: `core/src/remote/mod.rs:315` (clamp) and its test `set_sound_volume_message_is_dispatched_and_clamped`
- Modify: `src-tauri/src/state.rs:217` (`App::set_sound_volume` clamp)
- Modify: `src/components/Tile.tsx` (slider `max={2}`), `src/components/Tile.test.tsx`
- Modify: `core/src/remote/page.html:98` (`max="200"`)

**Interfaces:**
- Produces: `pub const MAX_SOUND_VOLUME: f32 = 2.0;` in `soundboard_core::config`.

- [ ] **Step 1: Failing tests**

In `core/src/config.rs` tests module add:

```rust
#[test]
fn sound_volume_keeps_up_to_200_percent() {
    let mut cfg = Config::default();
    for (id, v) in [("a", 1.5), ("b", 3.0), ("c", f32::NAN), ("d", -1.0)] {
        cfg.sounds.push(Sound { id: id.into(), name: id.into(), file: format!("{id}.wav"), image: None, volume: v, hotkey: None, color: None, trim: None, folder: None });
    }
    let cfg = cfg.sanitize();
    let vols: Vec<f32> = cfg.sounds.iter().map(|s| s.volume).collect();
    assert_eq!(vols, vec![1.5, MAX_SOUND_VOLUME, 1.0, 0.0]);
}
```

In `core/src/audio/engine.rs` tests add:

```rust
#[test]
fn sound_volume_goes_past_one_but_not_past_max() {
    let mut e = engine(both(), 0.5);
    e.play_buffer("a", 1.8, &clip()).unwrap();
    let h = e.active[0].handle;
    assert!((e.backend().gain(h).unwrap() - 0.9).abs() < 1e-6);
    e.set_sound_volume("a", 5.0);
    assert!((e.backend().gain(h).unwrap() - 1.0).abs() < 1e-6, "clamped to 2.0 × 0.5");
}
```

In `core/src/remote/mod.rs` change the test `set_sound_volume_message_is_dispatched_and_clamped` so the `u2` value 7.0 is expected to arrive as `2.0` (read the test's existing assertion and replace the expected `1.0` for `u2` with `2.0`). Also add `"value":1.5` for a new id `u4` expected as `1.5`.

In `src/components/Tile.test.tsx` add:

```tsx
it("lets the volume slider go to 200%", () => {
  // render a Tile as the other tests in this file do, with sound.volume = 1.5
  // then:
  const slider = screen.getByLabelText(/volume/);
  expect(slider).toHaveAttribute("max", "2");
  expect(screen.getByText("150%")).toBeInTheDocument();
});
```

(Follow the file's existing render helper for the Tile props.)

- [ ] **Step 2: Run, expect failures**

`cargo test -p soundboard-core volume` and `npx vitest run src/components/Tile.test.tsx` — fail (constant missing, clamps at 1).

- [ ] **Step 3: Implement**

`core/src/config.rs`:

```rust
/// Per-sound volume can boost a quiet clip up to 200%; master stays at most 100%.
pub const MAX_SOUND_VOLUME: f32 = 2.0;

/// Clamps to 0..=max; a non-finite value (hand-edited file) becomes `fallback`.
fn clamp_or(v: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, max) } else { fallback }
}
```

Replace `unit(...)` calls: `self.master_volume = clamp_or(self.master_volume, 1.0, default_master());` and `s.volume = clamp_or(s.volume, MAX_SOUND_VOLUME, 1.0);`. Delete `unit`.

`engine.rs`: `use crate::config::MAX_SOUND_VOLUME;` and in both `set_sound_volume` and `play_buffer` replace `.clamp(0.0, 1.0)` on the sound volume with `.clamp(0.0, MAX_SOUND_VOLUME)` (leave master clamps alone).

`remote/mod.rs:315`: `value.clamp(0.0, crate::config::MAX_SOUND_VOLUME)`.

`src-tauri/src/state.rs:217`: `let v = value.clamp(0.0, soundboard_core::config::MAX_SOUND_VOLUME);`.

`Tile.tsx`: `max={2}`. `page.html` line 98: `max="200"`.

- [ ] **Step 4: Run tests** — `cargo test -p soundboard-core`, `cargo test -p soundboard`, `npm test` all pass.

- [ ] **Step 5: Commit** — `feat(audio): per-sound volume up to 200%`

---

### Task 2: Soft limiter for gain above 1

**Files:**
- Modify: `core/src/audio/rodio_backend.rs` (`SharedSource`, `AudioThread`, `play`, `Cmd::SetGain`, `remove_sink`, `Cmd::Reset`, tests)

**Interfaces:**
- Produces: `fn soft_limit(x: f32) -> f32` (private, in rodio_backend.rs).
- Backend trait unchanged: `play(target, buffer, gain)` / `set_gain(handle, gain)` still take the combined linear gain.

- [ ] **Step 1: Failing tests** (in the file's `mod tests`)

```rust
#[test]
fn soft_limit_passes_quiet_samples_and_caps_loud_ones() {
    assert_eq!(soft_limit(0.5), 0.5);
    assert_eq!(soft_limit(-0.9), -0.9);
    for x in [1.0f32, 1.5, 4.0, 100.0] {
        let y = soft_limit(x);
        assert!(y > 0.9 && y < 1.0, "{x} -> {y}");
        assert_eq!(soft_limit(-x), -y);
    }
    assert!(soft_limit(1.5) > soft_limit(1.0), "monotonic");
}

#[test]
fn shared_source_applies_gain_and_limits_only_above_one() {
    let buffer = DecodedBuffer::new(1, 1000, vec![0.5, 0.95, -0.95]);
    let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
    let unity: Vec<f32> = SharedSource::new(&buffer, Arc::clone(&gain)).collect();
    assert_eq!(unity, vec![0.5, 0.95, -0.95], "gain 1 is bit-identical");
    gain.store(2.0f32.to_bits(), Ordering::Relaxed);
    let loud: Vec<f32> = SharedSource::new(&buffer, gain).collect();
    assert_eq!(loud[0], soft_limit(1.0));
    assert!(loud.iter().all(|s| s.abs() < 1.0));
}
```

Update the existing `shared_source_yields_the_buffer_without_copying` test to construct with `SharedSource::new(&buffer, Arc::new(AtomicU32::new(1.0f32.to_bits())))`.

- [ ] **Step 2: Run** `cargo test -p soundboard-core rodio_backend` — fails to compile (`soft_limit` missing, `new` arity).

- [ ] **Step 3: Implement**

```rust
use std::sync::atomic::{AtomicU32, Ordering};

/// Knee where the limiter starts bending samples toward ±1.
const KNEE: f32 = 0.9;

/// Soft knee: unchanged below `KNEE`, then approaches ±1 without reaching it.
fn soft_limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE { x } else { (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh()).copysign(x) }
}
```

`SharedSource` gets `gain: Arc<AtomicU32>` (f32 bits); `new(buffer, gain)`. `next()`:

```rust
let s = self.samples.get(self.pos).copied()?;
self.pos += 1;
let g = f32::from_bits(self.gain.load(Ordering::Relaxed));
// At or below unity the output is exactly what it was before boost existed.
Some(if g > 1.0 { soft_limit(s * g) } else { s * g })
```

`AudioThread` gets `gains: HashMap<SinkHandle, Arc<AtomicU32>>` (init empty). In `play`:

```rust
let sink = self.sink_for(target)?;
let g = Arc::new(AtomicU32::new(gain.to_bits()));
sink.append(SharedSource::new(buffer, Arc::clone(&g)));
let h = SinkHandle(self.next);
self.next += 1;
self.sinks.insert(h, sink);
self.gains.insert(h, g);
```

(remove `sink.set_volume(gain)` there; sink volume stays its default 1.0.) `Cmd::SetGain(h, g)`: `if let Some(a) = st.gains.get(&h) { a.store(g.to_bits(), Ordering::Relaxed); }`. `remove_sink`: also `self.gains.remove(&h);`. `Cmd::Reset`: also `st.gains.clear();`. The mic passthrough keeps `sink.set_volume`.

- [ ] **Step 4: Run** `cargo test -p soundboard-core` — passes.

- [ ] **Step 5: Commit** — `feat(audio): soft limiter when a sound plays above 100%`

---

### Task 3: Bass boost DSP

**Files:**
- Create: `core/src/audio/bass.rs`
- Modify: `core/src/audio/mod.rs` (add `pub mod bass;` following the existing module list style, and re-export `bass_boost` the way `trim` items are re-exported if they are)

**Interfaces:**
- Produces: `pub fn bass_boost(buffer: &DecodedBuffer, strength: f32) -> DecodedBuffer` — strength clamped to 0..=1; 0 (or non-finite) returns `buffer.clone()` sharing the `Arc`.

- [ ] **Step 1: Failing tests** (`bass.rs` `mod tests`)

```rust
use super::*;
use std::sync::Arc;

fn sine(freq: f32, amp: f32, channels: u16) -> DecodedBuffer {
    let rate = 44100;
    let samples = (0..rate).flat_map(|i| {
        let s = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
        std::iter::repeat(s).take(channels as usize)
    }).collect();
    DecodedBuffer::new(channels, rate as u32, samples)
}

fn rms(b: &DecodedBuffer) -> f32 {
    // skip the first 0.1 s so the filter has settled
    let skip = b.sample_rate as usize / 10 * b.channels as usize;
    let s = &b.samples[skip..];
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

#[test]
fn zero_strength_shares_the_samples() {
    let b = sine(50.0, 0.1, 1);
    assert!(Arc::ptr_eq(&bass_boost(&b, 0.0).samples, &b.samples));
    assert!(Arc::ptr_eq(&bass_boost(&b, f32::NAN).samples, &b.samples));
}

#[test]
fn boosts_lows_far_more_than_highs() {
    let (low, high) = (sine(50.0, 0.1, 1), sine(5000.0, 0.1, 1));
    let low_gain = rms(&bass_boost(&low, 1.0)) / rms(&low);
    let high_gain = rms(&bass_boost(&high, 1.0)) / rms(&high);
    assert!(low_gain > 2.0 * high_gain, "low {low_gain} high {high_gain}");
}

#[test]
fn never_exceeds_full_scale() {
    for strength in [0.3, 1.0] {
        let out = bass_boost(&sine(60.0, 1.0, 2), strength);
        assert!(out.samples.iter().all(|s| s.abs() <= 1.0 + 1e-6));
    }
}

#[test]
fn keeps_format_and_filters_channels_independently() {
    let mut samples = Vec::new();
    for i in 0..44100 {
        let t = i as f32 / 44100.0;
        samples.push(0.1 * (2.0 * std::f32::consts::PI * 50.0 * t).sin()); // left: bass
        samples.push(0.0); // right: silence
    }
    let out = bass_boost(&DecodedBuffer::new(2, 44100, samples), 1.0);
    assert_eq!((out.channels, out.sample_rate, out.samples.len()), (2, 44100, 88200));
    assert!(out.samples.iter().skip(1).step_by(2).all(|s| *s == 0.0), "silent right stays silent");
}
```

- [ ] **Step 2: Run** `cargo test -p soundboard-core bass` — fails (module missing).

- [ ] **Step 3: Implement** `core/src/audio/bass.rs`:

```rust
//! Non-destructive bass boost: a low-shelf filter plus soft saturation, applied to a decoded buffer.

use super::backend::DecodedBuffer;

const SHELF_HZ: f64 = 120.0;
const MAX_BOOST_DB: f64 = 18.0;

/// Bass-boosted copy of `buffer`. `strength` 0..=1 scales the shelf gain (up to +18 dB) and the
/// saturation drive; the result is scaled down if needed so its peak is at most 1.0.
/// Strength 0 (or non-finite) returns `buffer` itself, sharing its samples.
pub fn bass_boost(buffer: &DecodedBuffer, strength: f32) -> DecodedBuffer {
    let strength = if strength.is_finite() { strength.clamp(0.0, 1.0) } else { 0.0 };
    if strength == 0.0 {
        return buffer.clone();
    }
    let channels = buffer.channels.max(1) as usize;
    let coeffs = low_shelf(buffer.sample_rate.max(1) as f64, MAX_BOOST_DB * strength as f64);
    let drive = 1.0 + 3.0 * strength;
    let norm = drive.tanh();
    let mut state = vec![[0.0f64; 4]; channels]; // x1, x2, y1, y2 per channel
    let mut out: Vec<f32> = buffer.samples.iter().enumerate().map(|(i, &x)| {
        let st = &mut state[i % channels];
        let x = x as f64;
        let y = coeffs[0] * x + coeffs[1] * st[0] + coeffs[2] * st[1] - coeffs[3] * st[2] - coeffs[4] * st[3];
        *st = [x, st[0], y, st[2]];
        (drive * y as f32).tanh() / norm
    }).collect();
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 1.0 {
        out.iter_mut().for_each(|s| *s /= peak);
    }
    DecodedBuffer::new(buffer.channels, buffer.sample_rate, out)
}

/// RBJ cookbook low shelf (slope 1), normalized: [b0, b1, b2, a1, a2].
fn low_shelf(rate: f64, gain_db: f64) -> [f64; 5] {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * SHELF_HZ / rate;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / 2.0 * std::f64::consts::SQRT_2;
    let k = 2.0 * a.sqrt() * alpha;
    let b0 = a * ((a + 1.0) - (a - 1.0) * cos + k);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos - k);
    let a0 = (a + 1.0) + (a - 1.0) * cos + k;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos);
    let a2 = (a + 1.0) + (a - 1.0) * cos - k;
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}
```

- [ ] **Step 4: Run** `cargo test -p soundboard-core bass` — passes. If `boosts_lows_far_more_than_highs` fails, check the coefficient signs against the RBJ cookbook before changing the threshold.

- [ ] **Step 5: Commit** — `feat(audio): bass boost DSP`

---

### Task 4: Bass boost in config, engine and commands

**Files:**
- Modify: `core/src/config.rs` (`Sound.bass`, sanitize, tests, struct literals)
- Modify: `core/src/library.rs`, `core/src/hotkeys.rs` (every `Sound { .. }` literal gets `bass: 0.0`)
- Modify: `core/src/audio/engine.rs` (cache key, `play_file`, `preview`, `set_bass`, tests)
- Modify: `src-tauri/src/state.rs` (`UiSound.bass`, `play_file` call), `src-tauri/src/commands.rs` (`set_bass`, `preview_trim`), `src-tauri/src/lib.rs` (register `commands::set_bass`)

**Interfaces:**
- Consumes: `bass_boost(&DecodedBuffer, f32) -> DecodedBuffer` (Task 3).
- Produces: `Sound.bass: f32`; `Engine::play_file(&mut self, sound_id: &str, sound_volume: f32, path: &Path, trim: Option<Trim>, bass: f32)`; `Engine::preview(&mut self, sound_id: &str, path: &Path, trim: Trim, sound_volume: f32, bass: f32)`; `Engine::set_bass(&mut self, sound_id: &str, bass: f32)`; Tauri command `set_bass(id: String, value: f32) -> Result<(), String>`; `UiSound.bass: f32`.

- [ ] **Step 1: Failing tests**

`config.rs` tests:

```rust
#[test]
fn bass_defaults_to_off_and_is_clamped() {
    let json = r#"{"sounds":[{"id":"a","name":"A","file":"a.wav"},{"id":"b","name":"B","file":"b.wav","bass":4.0},{"id":"c","name":"C","file":"c.wav","bass":0.4}]}"#;
    let cfg: Config = serde_json::from_str(json).unwrap();
    let cfg = cfg.sanitize();
    let bass: Vec<f32> = cfg.sounds.iter().map(|s| s.bass).collect();
    assert_eq!(bass, vec![0.0, 1.0, 0.4]);
}
```

`engine.rs` tests (next to `trimmed_slice_is_reused_until_the_trim_changes`):

```rust
#[test]
fn bass_slice_is_reused_until_the_bass_changes() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("a.wav");
    write_test_wav(&p, 1.0); // use the same helper/signature the neighbouring trim tests use
    let mut e = engine(both(), 1.0);
    e.play_file("a", 1.0, &p, None, 0.5).unwrap();
    let first = e.trimmed.get("a").unwrap().2.samples.clone();
    e.play_file("a", 1.0, &p, None, 0.5).unwrap();
    assert!(Arc::ptr_eq(&first, &e.trimmed.get("a").unwrap().2.samples), "no second pass");
    e.set_bass("a", 0.5);
    assert!(e.trimmed.contains_key("a"), "same bass keeps it");
    e.set_bass("a", 0.8);
    assert!(!e.trimmed.contains_key("a"), "changed bass drops it");
    e.play_file("a", 1.0, &p, None, 0.0).unwrap();
    assert!(!e.trimmed.contains_key("a"), "plain play keeps nothing extra");
}
```

(Copy the exact wav-writing setup from `trimmed_slice_is_reused_until_the_trim_changes`.)

- [ ] **Step 2: Run** `cargo test -p soundboard-core` — fails to compile.

- [ ] **Step 3: Implement**

`config.rs` `Sound` after `trim`:

```rust
    /// Bass boost strength 0..=1; 0 is off. Applied after the trim, never to the file.
    #[serde(default)]
    pub bass: f32,
```

sanitize loop: `s.bass = clamp_or(s.bass, 1.0, 0.0);`. Add `bass: 0.0` to every `Sound { .. }` literal (`grep -rn "Sound {" core src-tauri/src`; library.rs `import_sound` too).

`engine.rs`:
- `use super::bass::bass_boost;`
- field: `trimmed: HashMap<String, (Option<Trim>, f32, DecodedBuffer)>` with doc "Per sound id: the trim and bass a processed slice was made for, and that slice."
- `play_file(.., trim, bass)` calls `self.trimmed_buffer(sound_id, &full, trim, bass)`.
- `trimmed_buffer`:

```rust
fn trimmed_buffer(&mut self, sound_id: &str, full: &DecodedBuffer, trim: Option<Trim>, bass: f32) -> DecodedBuffer {
    let cache = &self.cache;
    self.trimmed.retain(|id, _| cache.contains(id));
    if trim.is_none() && bass == 0.0 {
        self.trimmed.remove(sound_id);
        return full.clone();
    }
    if let Some((t, b, slice)) = self.trimmed.get(sound_id) {
        if *t == trim && *b == bass {
            return slice.clone();
        }
    }
    let slice = bass_boost(&apply_trim(full, trim), bass);
    self.trimmed.insert(sound_id.to_string(), (trim, bass, slice.clone()));
    slice
}
```

- `set_trim`: `if self.trimmed.get(sound_id).is_some_and(|(t, _, _)| *t != trim) { self.trimmed.remove(sound_id); }`
- new:

```rust
/// Forgets the processed slice when the bass changed; the next play builds it again.
pub fn set_bass(&mut self, sound_id: &str, bass: f32) {
    if self.trimmed.get(sound_id).is_some_and(|(_, b, _)| *b != bass) {
        self.trimmed.remove(sound_id);
    }
}
```

- `preview(.., trim, sound_volume, bass)`: `let slice = bass_boost(&apply_trim(&full, Some(trim)), bass);`
- Update existing tests: `play_file(.., X)` → `play_file(.., X, 0.0)`, `preview(.., 0.8)` → `preview(.., 0.8, 0.0)`, `.1.samples` → `.2.samples` on `trimmed` entries.

`src-tauri/src/state.rs:277`: `.play_file(&sound.id, sound.volume, &path, sound.trim, sound.bass)`; `UiSound` gets `pub bass: f32,` filled with `bass: s.bass`.

`src-tauri/src/commands.rs`: `preview_trim` passes `sound.bass` as the last arg. New command next to `set_trim`:

```rust
#[tauri::command]
pub fn set_bass(id: String, value: f32, app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
    mutate(&ctx, |a| {
        a.config.sound_mut(&id).ok_or("That sound no longer exists")?.bass = value;
        a.engine.set_bass(&id, value);
        a.mark_dirty();
        Ok(())
    })?;
    emit_state(&app, &ctx);
    Ok(())
}
```

Register `commands::set_bass,` in `src-tauri/src/lib.rs` next to `commands::set_trim,`.

- [ ] **Step 4: Run** `cargo test -p soundboard-core` and `cargo test -p soundboard` — pass.

- [ ] **Step 5: Commit** — `feat(audio): per-sound bass boost saved in the config`

---

### Task 5: Bass boost UI

**Files:**
- Create: `src/components/BassPopover.tsx`, `src/components/BassPopover.test.tsx`
- Modify: `src/lib/api.ts` (`UiSound.bass`, `setBass`), `src/components/TileGrid.tsx` (menu item + popover), `src/components/TileGrid.test.tsx`
- Modify: every `UiSound` fixture in `src/**/*.test.ts(x)` (add `bass: 0`): `Tile.test.tsx`, `TileGrid.test.tsx`, `TrimEditor.test.tsx`, `src/lib/view.test.ts`

**Interfaces:**
- Consumes: Tauri command `set_bass(id, value)`; `UiSound.bass`.
- Produces: `api.setBass(id: string, value: number): Promise<void>`; `BassPopover({ x, y, name, value, onCommit, onClose })`.

- [ ] **Step 1: Failing tests**

`src/components/BassPopover.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { BassPopover } from "./BassPopover";

describe("BassPopover", () => {
  it("shows the strength and commits on release", () => {
    const onCommit = vi.fn();
    render(<BassPopover x={10} y={10} name="Airhorn" value={0.4} onCommit={onCommit} onClose={() => {}} />);
    expect(screen.getByText("40%")).toBeInTheDocument();
    const slider = screen.getByLabelText("Airhorn bass boost");
    fireEvent.change(slider, { target: { value: "0.75" } });
    expect(screen.getByText("75%")).toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    expect(onCommit).toHaveBeenCalledWith(0.75);
  });
});
```

In `TileGrid.test.tsx` add `setBass: vi.fn()` to the mocked `api`, `bass: 0` to `snd`, and:

```tsx
it("opens the bass boost slider from the context menu", () => {
  setup([snd("a", null)], []);
  fireEvent.contextMenu(screen.getByRole("button", { name: "A" }));
  fireEvent.click(screen.getByText("Bass boost…"));
  const slider = screen.getByLabelText("A bass boost");
  fireEvent.change(slider, { target: { value: "0.5" } });
  fireEvent.pointerUp(slider);
  expect(api.setBass).toHaveBeenCalledWith("a", 0.5);
});
```

- [ ] **Step 2: Run** `npx vitest run src/components` — fails.

- [ ] **Step 3: Implement**

`api.ts`: in `UiSound` add `/** Bass boost strength 0..1; 0 is off. */ bass: number;` and in `api`: `setBass: (id: string, value: number) => invoke<void>("set_bass", { id, value }),`.

`src/components/BassPopover.tsx`:

```tsx
import { usePopoverPosition } from "../lib/usePopoverPosition";
import { useSliderDraft } from "../lib/useSliderDraft";

interface Props { x: number; y: number; name: string; value: number; onCommit: (v: number) => void; onClose: () => void }

/** Bass boost strength for one sound; saves when the slider is released. */
export function BassPopover({ x, y, name, value, onCommit, onClose }: Props) {
  const { ref, pos } = usePopoverPosition(x, y, onClose);
  const bass = useSliderDraft(value, onCommit);
  return (
    <div
      ref={ref}
      role="dialog"
      aria-label="Bass boost"
      style={{ left: pos.x, top: pos.y }}
      className="fixed z-50 flex w-60 items-center gap-2 rounded-md border border-border bg-surface p-3 shadow-xl"
    >
      <span className="text-[12px] text-dim">Bass</span>
      <input type="range" min={0} max={1} step={0.05} {...bass} aria-label={`${name} bass boost`} className="min-w-0 flex-1" />
      <span className="w-9 text-right font-mono text-[11px] tabular-nums text-dim">{Math.round(bass.value * 100)}%</span>
    </div>
  );
}
```

`TileGrid.tsx`: state `const [bassing, setBassing] = useState<Menu | null>(null);`; menu item after the trim item:

```tsx
{ label: s.bass > 0 ? `Bass boost (${Math.round(s.bass * 100)}%)` : "Bass boost…", onSelect: () => menu && setBassing({ sound: s, x: menu.x, y: menu.y }) },
```

render next to the `ColorPicker` block:

```tsx
{bassing && (
  <BassPopover
    x={bassing.x}
    y={bassing.y}
    name={bassing.sound.name}
    value={bassing.sound.bass}
    onCommit={(v) => void run(api.setBass(bassing.sound.id, v))}
    onClose={() => setBassing(null)}
  />
)}
```

Note the menu label when on is "Bass boost (50%)"; the test above uses a sound with `bass: 0` so it clicks "Bass boost…".

Add `bass: 0` to every `UiSound` fixture (`npm run build` type-checks; fix every error it lists).

- [ ] **Step 4: Run** `npm test` and `npm run build` — pass.

- [ ] **Step 5: Commit** — `feat(ui): bass boost slider in the tile menu`

---

### Task 6: Link downloader core (`core/src/link.rs`)

**Files:**
- Create: `core/src/link.rs`
- Modify: `core/src/lib.rs` (`pub mod link;`), `core/Cargo.toml` (add `"Win32_System_Registry"` to the windows features)

**Interfaces:**
- Produces:
  - `pub const MAX_LINK_SECS: u32 = 600;`
  - `pub fn is_link(url: &str) -> bool`
  - `pub fn find_in_dirs(dirs: &[PathBuf], exe: &str) -> Option<PathBuf>`
  - `pub fn find_tool(exe: &str) -> Option<PathBuf>` (exe like `"yt-dlp.exe"`)
  - `pub fn ytdlp_args(url: &str, ffmpeg: &Path, out_dir: &Path) -> Vec<OsString>`
  - `pub fn error_line(stderr: &str) -> String`
  - `pub fn download_audio(url: &str, out_dir: &Path) -> Result<PathBuf, String>` — blocking; returns the mp3 path inside `out_dir`.

- [ ] **Step 1: Failing tests** (`link.rs` `mod tests`)

```rust
use super::*;
use tempfile::tempdir;

#[test]
fn only_http_links_count() {
    assert!(is_link("https://youtu.be/abc"));
    assert!(is_link("http://example.com/a"));
    assert!(!is_link("ftp://x"));
    assert!(!is_link("C:\\a.mp3"));
    assert!(!is_link(""));
}

#[test]
fn finds_the_first_dir_holding_the_exe() {
    let (a, b) = (tempdir().unwrap(), tempdir().unwrap());
    std::fs::write(b.path().join("yt-dlp.exe"), b"").unwrap();
    let dirs = vec![a.path().to_path_buf(), b.path().to_path_buf()];
    assert_eq!(find_in_dirs(&dirs, "yt-dlp.exe"), Some(b.path().join("yt-dlp.exe")));
    assert_eq!(find_in_dirs(&dirs, "ffmpeg.exe"), None);
}

#[test]
fn args_extract_mp3_with_cap_and_no_playlist() {
    let args: Vec<String> = ytdlp_args("https://x.test/v", Path::new("C:/ff/ffmpeg.exe"), Path::new("C:/tmp/o"))
        .into_iter().map(|a| a.to_string_lossy().into_owned()).collect();
    for want in ["--no-playlist", "-x", "--ignore-config", "--no-progress"] {
        assert!(args.contains(&want.to_string()), "{want}");
    }
    let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    assert_eq!(after("--audio-format"), "mp3");
    assert_eq!(after("--match-filter"), "duration<=?600");
    assert_eq!(after("--ffmpeg-location"), "C:/ff/ffmpeg.exe");
    assert!(after("-o").starts_with("C:/tmp/o"));
    assert_eq!(args.last().unwrap(), "https://x.test/v");
}

#[test]
fn error_line_is_the_last_yt_dlp_error() {
    let stderr = "WARNING: x\nERROR: [youtube] abc: Video unavailable\nERROR: [instagram] Requested content is not available, rate-limit reached or login required\n";
    assert_eq!(error_line(stderr), "[instagram] Requested content is not available, rate-limit reached or login required");
    assert_eq!(error_line("garbage"), "Download failed");
}
```

- [ ] **Step 2: Run** `cargo test -p soundboard-core link` — fails (module missing).

- [ ] **Step 3: Implement** `core/src/link.rs`:

```rust
//! Add a sound from a link: finds yt-dlp and ffmpeg (installed by winget) and runs yt-dlp.

use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Longest video accepted: sounds are fully decoded into memory.
pub const MAX_LINK_SECS: u32 = 600;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(120);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn is_link(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

pub fn find_in_dirs(dirs: &[PathBuf], exe: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Process PATH, then user and machine PATH read fresh from the registry (winget edits them
/// after this process started), then winget's Links folder.
pub fn find_tool(exe: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for p in registry_paths() {
        dirs.extend(std::env::split_paths(&p));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links"));
    }
    find_in_dirs(&dirs, exe)
}

#[cfg(windows)]
fn registry_paths() -> Vec<OsString> {
    use std::os::windows::ffi::OsStringExt;
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let read = |key, sub| -> Option<OsString> {
        let mut len = 0u32;
        // RRF_RT_REG_SZ alone also accepts REG_EXPAND_SZ and expands %VARS% for us.
        unsafe { RegGetValueW(key, sub, w!("Path"), RRF_RT_REG_SZ, None, None, Some(&mut len)) }.ok().ok()?;
        let mut buf = vec![0u16; (len as usize).div_ceil(2)];
        unsafe { RegGetValueW(key, sub, w!("Path"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) }.ok().ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(OsString::from_wide(&buf[..end]))
    };
    [
        read(HKEY_CURRENT_USER, w!("Environment")),
        read(HKEY_LOCAL_MACHINE, w!(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(not(windows))]
fn registry_paths() -> Vec<OsString> {
    Vec::new()
}

pub fn ytdlp_args(url: &str, ffmpeg: &Path, out_dir: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--ignore-config", "--no-playlist", "--no-progress", "-x", "--audio-format", "mp3",
        "--match-filter", &format!("duration<=?{MAX_LINK_SECS}"), "--ffmpeg-location",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(ffmpeg.into());
    args.push("-o".into());
    args.push(out_dir.join("%(title).80B.%(ext)s").into());
    args.push(url.into());
    args
}

pub fn error_line(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("ERROR: "))
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| "Download failed".to_string())
}

fn hidden(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs to completion or kills it after `timeout`; stderr goes to `stderr_file`. Ok(true) on exit code 0.
fn run(mut cmd: Command, stderr_file: &Path, timeout: Duration) -> Result<bool, String> {
    let err = File::create(stderr_file).map_err(|e| e.to_string())?;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(err).spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok(status.success());
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The download took too long and was stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn first_mp3(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp3")))
}

fn is_winget(p: &Path) -> bool {
    p.to_string_lossy().to_ascii_lowercase().contains(r"\microsoft\winget\")
}

/// Downloads `url` as an mp3 into `out_dir` (which must exist). A failure with a winget-installed
/// yt-dlp upgrades it once and retries, since an outdated yt-dlp is the usual cause.
pub fn download_audio(url: &str, out_dir: &Path) -> Result<PathBuf, String> {
    let ytdlp = find_tool("yt-dlp.exe").ok_or("yt-dlp is not installed. Install it with: winget install -e --id yt-dlp.yt-dlp")?;
    let ffmpeg = find_tool("ffmpeg.exe").ok_or("ffmpeg is not installed. Install it with: winget install -e --id Gyan.FFmpeg")?;
    let stderr = out_dir.join("yt-dlp.log");
    let attempt = || -> Result<Option<PathBuf>, String> {
        let mut cmd = hidden(&ytdlp);
        cmd.args(ytdlp_args(url, &ffmpeg, out_dir));
        if !run(cmd, &stderr, DOWNLOAD_TIMEOUT)? {
            return Ok(None);
        }
        // Exit 0 without a file: the match filter skipped a video longer than the cap.
        first_mp3(out_dir).map(Some).ok_or_else(|| format!("Videos longer than {} minutes are not supported", MAX_LINK_SECS / 60))
    };
    if let Some(mp3) = attempt()? {
        return Ok(mp3);
    }
    if is_winget(&ytdlp) {
        if let Some(winget) = find_tool("winget.exe") {
            let mut cmd = hidden(&winget);
            cmd.args(["upgrade", "-e", "--id", "yt-dlp.yt-dlp", "--accept-source-agreements", "--accept-package-agreements"]);
            let _ = run(cmd, &out_dir.join("winget.log"), UPGRADE_TIMEOUT);
            if let Some(mp3) = attempt()? {
                return Ok(mp3);
            }
        }
    }
    Err(error_line(&fs::read_to_string(&stderr).unwrap_or_default()))
}
```

Note: the `--ffmpeg-location` value is the ffmpeg.exe path (yt-dlp accepts a file or its folder). `windows::core::Result::ok()` returns `Result<(), Error>` from `WIN32_ERROR`; if the signature in windows 0.62 differs, adapt so a failed read returns `None` — do not panic. If `CREATE_NO_WINDOW` triggers an unused warning on non-Windows, move it inside the `cfg(windows)` block.

- [ ] **Step 4: Run** `cargo test -p soundboard-core link` and `cargo clippy -p soundboard-core` (if available) — pass, no warnings.

- [ ] **Step 5: Commit** — `feat(link): find yt-dlp/ffmpeg and download a link as mp3`

---

### Task 7: `add_sound_from_url` command and "From link" UI

**Files:**
- Modify: `src-tauri/src/commands.rs` (new command, shared insert helper), `src-tauri/src/lib.rs` (register)
- Modify: `src/lib/api.ts`, `src/lib/store.ts` (`linkBusy`), `src/lib/actions.ts` (`addFromLink`), `src/lib/actions.test.ts`
- Modify: `src/components/Dialogs.tsx` (`RenameDialog` gets optional `submitLabel`), `src/components/TopBar.tsx`, `src/components/TopBar.test.tsx`

**Interfaces:**
- Consumes: `soundboard_core::link::{is_link, download_audio}`; `Library::import_sound`.
- Produces: Tauri command `add_sound_from_url(url: String, folder: Option<String>) -> Result<String, String>` (new sound id); `api.addSoundFromUrl(url: string, folder: string | null): Promise<string>`; `addFromLink(url: string): Promise<void>`; store `linkBusy: boolean`.

- [ ] **Step 1: Failing tests**

`src/lib/actions.test.ts`: add `addSoundFromUrl: vi.fn()` to the mocked api and:

```ts
describe("addFromLink", () => {
  const addSoundFromUrl = api.addSoundFromUrl as ReturnType<typeof vi.fn>;
  beforeEach(() => {
    vi.clearAllMocks();
    useStore.setState({ toasts: [], pendingTrimId: null, openFolder: "f1", linkBusy: false });
  });

  it("downloads into the open folder, opens the trim editor and clears busy", async () => {
    addSoundFromUrl.mockResolvedValue("s9");
    const p = addFromLink("https://youtu.be/x");
    expect(useStore.getState().linkBusy).toBe(true);
    await p;
    expect(addSoundFromUrl).toHaveBeenCalledWith("https://youtu.be/x", "f1");
    expect(useStore.getState().pendingTrimId).toBe("s9");
    expect(useStore.getState().linkBusy).toBe(false);
  });

  it("toasts the error and clears busy", async () => {
    addSoundFromUrl.mockRejectedValue("Video unavailable");
    await addFromLink("https://youtu.be/x");
    expect(useStore.getState().toasts[0]).toMatchObject({ level: "error", message: "Video unavailable" });
    expect(useStore.getState().linkBusy).toBe(false);
  });
});
```

`src/components/TopBar.test.tsx`: change the actions mock to `({ pickAndAddSounds: vi.fn(), addFromLink: vi.fn() })`, import `addFromLink` from `../lib/actions`, set `linkBusy: false` in `beforeEach`, and add:

```tsx
it("adds a sound from a pasted link", () => {
  render(<TopBar />);
  fireEvent.click(screen.getByText("From link"));
  fireEvent.change(screen.getByLabelText("Paste a link"), { target: { value: " https://youtu.be/x " } });
  fireEvent.click(screen.getByText("Add"));
  expect(addFromLink).toHaveBeenCalledWith("https://youtu.be/x");
});

it("shows Downloading while a link download runs", () => {
  useStore.setState({ linkBusy: true });
  render(<TopBar />);
  expect(screen.getByText("Downloading…")).toBeDisabled();
});
```

Note: the dialog's submit button is labelled "Add", and so is the top bar's existing "Add" button; scope the click with `within(screen.getByRole("dialog"))` from `@testing-library/react` if `getByText("Add")` finds two.

- [ ] **Step 2: Run** `npm test` — fails.

- [ ] **Step 3: Implement (Rust)**

In `commands.rs`, extract the block of `add_sounds` that inserts imported sounds into a helper and use it from both commands:

```rust
/// Adds freshly imported sounds to the config in `folder` (top level when that folder is gone),
/// saves, and starts decoding them in the background.
fn insert_imported(ctx: &Ctx, imported: Vec<Sound>, folder: Option<String>) {
    let ids: Vec<String> = imported.iter().map(|s| s.id.clone()).collect();
    {
        let mut a = lock(&ctx.shared);
        let folder = folder.filter(|f| a.config.folder(f).is_some());
        a.config.sounds.extend(imported.into_iter().map(|mut s| {
            s.folder = folder.clone();
            s
        }));
        a.mark_dirty();
        a.refresh_files();
    }
    spawn_warm(ctx.shared.clone(), Some(ids));
}
```

(`add_sounds` keeps its behaviour: `if !imported.is_empty() { insert_imported(&ctx, imported, folder); }`.) Also extract the "learn library root and used colors" lock block into `fn library_root_and_colors(ctx: &Ctx) -> (PathBuf, Vec<String>)` and use it in both.

New command:

```rust
/// Downloads the audio of `url` with yt-dlp into a temp folder, imports it as one sound and
/// returns its id. The temp folder is removed whether or not it worked.
#[tauri::command]
pub async fn add_sound_from_url(url: String, folder: Option<String>, app: AppHandle, ctx: State<'_, Ctx>) -> Result<String, String> {
    let url = url.trim().to_string();
    if !link::is_link(&url) {
        return Err("That is not a link".to_string());
    }
    let (root, used_colors) = library_root_and_colors(&ctx);
    let sound = tauri::async_runtime::spawn_blocking(move || -> Result<Sound, String> {
        let tmp = std::env::temp_dir().join(format!("soundboard-link-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let result = link::download_audio(&url, &tmp).and_then(|mp3| {
            let library = Library::open(root).map_err(|e| format!("Could not open the sound library: {e}"))?;
            library.import_sound(&mp3, &used_colors.iter().map(String::as_str).collect::<Vec<_>>()).map_err(|e| e.to_string())
        });
        let _ = fs::remove_dir_all(&tmp);
        result
    })
    .await
    .map_err(|e| format!("Downloading failed: {e}"))??;
    let id = sound.id.clone();
    insert_imported(&ctx, vec![sound], folder);
    emit_state(&app, &ctx);
    Ok(id)
}
```

Add needed imports (`soundboard_core::link`, `std::fs`, `uuid` — if `uuid` is not a src-tauri dependency, add `uuid = { version = "1", features = ["v4"] }` to `src-tauri/Cargo.toml`, matching core's version). Register `commands::add_sound_from_url,` in `lib.rs`.

- [ ] **Step 4: Implement (UI)**

`api.ts`: `addSoundFromUrl: (url: string, folder: string | null) => invoke<string>("add_sound_from_url", { url, folder }),`

`store.ts`: interface `/** A link download is running; the From link button shows Downloading…. */ linkBusy: boolean;` and initial `linkBusy: false,`.

`actions.ts`:

```ts
/** Downloads a link's audio as a new sound in the open folder and opens the trim editor for it. */
export async function addFromLink(url: string) {
  const { toast, setPendingTrim, openFolder } = useStore.getState();
  useStore.setState({ linkBusy: true });
  try {
    setPendingTrim(await api.addSoundFromUrl(url, openFolder));
  } catch (e) {
    toast("error", String(e));
  } finally {
    useStore.setState({ linkBusy: false });
  }
}
```

`Dialogs.tsx` `RenameDialog`: add `submitLabel?: string` to `RenameProps`, default `"Save"`, render `{submitLabel}` in the submit button.

`TopBar.tsx`: `const linkBusy = useStore((s) => s.linkBusy); const [pasting, setPasting] = useState(false);` and before the `Add` button:

```tsx
<button type="button" onClick={() => setPasting(true)} disabled={linkBusy} className="btn" title="Add a sound from a YouTube, Instagram, Facebook or other link">
  {linkBusy ? "Downloading…" : "From link"}
</button>
```

after the header content (inside the fragment/return, e.g. right before `</header>`):

```tsx
{pasting && (
  <RenameDialog
    title="Paste a link"
    initial=""
    submitLabel="Add"
    onCancel={() => setPasting(false)}
    onSubmit={(url) => { setPasting(false); void addFromLink(url); }}
  />
)}
```

(`RenameDialog` already trims the value and blocks empty submits. Its `<label htmlFor="rename">` gives the input the accessible name "Paste a link".)

- [ ] **Step 5: Run** `cargo test -p soundboard`, `npm test`, `npm run build` — pass.

- [ ] **Step 6: Commit** — `feat: add a sound from a link`

---

### Task 8: Installer prompt, README, full verification

**Files:**
- Create: `src-tauri/installer-hooks.nsh`
- Modify: `src-tauri/tauri.conf.json` (`bundle.windows.nsis.installerHooks`), `README.md`

- [ ] **Step 1: NSIS hook** `src-tauri/installer-hooks.nsh`:

```nsis
; Offers to install yt-dlp and ffmpeg (for "From link") with winget after Soundboard installs.
; Skipped on silent installs, when winget is missing, and when both tools are already found.
!macro NSIS_HOOK_POSTINSTALL
  IfSilent sb_link_done
  nsExec::Exec 'cmd /c where winget'
  Pop $0
  StrCmp $0 0 0 sb_link_done
  nsExec::Exec 'cmd /c where yt-dlp && where ffmpeg'
  Pop $0
  StrCmp $0 0 sb_link_done
  MessageBox MB_YESNO|MB_ICONQUESTION "Install yt-dlp and ffmpeg so Soundboard can add sounds from links (YouTube, Instagram, Facebook and more)?$\r$\n$\r$\nThis uses winget and downloads about 100 MB. You can skip this and install them later." IDNO sb_link_done
  ExecWait 'winget install -e --id yt-dlp.yt-dlp --accept-source-agreements --accept-package-agreements'
  ExecWait 'winget install -e --id Gyan.FFmpeg --accept-source-agreements --accept-package-agreements'
  sb_link_done:
!macroend
```

`tauri.conf.json` → in `bundle.windows.nsis` add `"installerHooks": "./installer-hooks.nsh"`.

- [ ] **Step 2: README** — add sections (match the README's existing heading style and tone):
  - "Add a sound from a link": click **From link**, paste a URL; needs yt-dlp and ffmpeg (the installer offers to install them with winget, or run `winget install -e --id yt-dlp.yt-dlp` and `winget install -e --id Gyan.FFmpeg`); videos up to 10 minutes; the sound goes into the open folder; on failure the app updates a winget-installed yt-dlp once and retries; errors show yt-dlp's message (private, login required, region-blocked, unsupported site). Keep yt-dlp current with `winget upgrade yt-dlp.yt-dlp`.
  - "Bass boost": right-click a tile → **Bass boost…**, 0–100%, applies from the next play, the file is untouched.
  - Volume: per-sound sliders go to 200% (quiet clips); above 100% a soft limiter prevents harsh clipping; master stays at 100%.

- [ ] **Step 3: Full verification**

```bash
cargo test -p soundboard-core
cargo test -p soundboard
npm test
npm run build
```

All must pass. Then build the installer in the background to prove the NSIS hook compiles: `npm run tauri build` (do NOT run the resulting installer; never stop the installed app).

- [ ] **Step 4: Commit** — `feat(installer): offer yt-dlp and ffmpeg via winget; docs: README`
