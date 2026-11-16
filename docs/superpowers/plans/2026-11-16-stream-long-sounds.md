# Stream long sounds — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Sounds whose play length is over 30 s play straight from the file instead of being cached decoded.

**Architecture:** The backend gains `play_stream` (rodio `Decoder` → trim → streaming bass → the shared gain/limiter stage). The engine learns each file's length (probe or decode) and routes long plays to `play_stream`.

**Tech Stack:** Rust (rodio 0.20.1), Tauri 2.

Spec: `docs/superpowers/specs/2026-11-16-stream-long-sounds-design.md`.

## Global Constraints

- Branch `feat/ram-cache` (continues the RAM work); never commit to `master`.
- Commit messages end with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `pub const STREAM_SECS: f32 = 30.0;` in `core/src/audio/engine.rs`.
- Play length: `trim.map(|t| t.end_secs.min(full) - t.start_secs).unwrap_or(full)`; stream when it is `> STREAM_SECS`.
- Gain at ≤ 1 stays exactly `x * gain` with no limiter, same as `SharedSource`; above 1 uses `soft_limit`. Streamed bass output always goes through `soft_limit`.
- No config change. Decoding stays outside the App lock where it is today (warming, trim editor).
- Verify with `cargo test -p soundboard-core`, `cargo test -p soundboard`, `npm test`, `npm run build`.
- Never run, kill or install the app.

---

### Task 1: Streaming playback in the backend

**Files:** `core/src/audio/bass.rs`, `core/src/audio/backend.rs` (trait), `core/src/audio/rodio_backend.rs`, `core/src/audio/fake_backend.rs`, `core/src/audio/decode.rs`.

**Interfaces:**
- `bass.rs`: `pub struct BassFilter` with `pub fn new(channels: u16, sample_rate: u32, strength: f32) -> Option<Self>` (None when strength is 0 or non-finite) and `pub fn process(&mut self, sample: f32) -> f32` (interleaved: tracks the channel index itself). `bass_boost` is rewritten to use it, then keeps its peak normalization. Existing bass tests stay green unchanged.
- `decode.rs`: `pub fn probe_secs(path: &Path) -> Option<f32>` — opens a `Decoder`, returns `total_duration()` in seconds, never decodes.
- `backend.rs` trait: `fn play_stream(&self, target: &DeviceTarget, path: &Path, trim: Option<Trim>, bass: f32, gain: f32) -> Result<SinkHandle, AudioError>;` Same handle semantics as `play` (`stop`, `set_gain`, `is_finished` work on it).
- `rodio_backend.rs`: refactor the gain/limiter stage out of `SharedSource` into a generic source wrapper used by both buffer and stream playback (no duplicated gain code). New `Cmd::PlayStream`. The stream: `Decoder::new(BufReader::new(File::open(path)?))`, `.convert_samples::<f32>()`, seek to `trim.start_secs` with `try_seek` (fall back to `skip_duration` if it errors), `take_duration(end - start)` when trimmed, then `BassFilter` + `soft_limit` when bass > 0, then the gain stage. Open/decoder errors become `AudioError::Decode`/`Io`.
- `fake_backend.rs`: record `Call::PlayStream { target, path, trim, bass, gain }` and return handles like `play`.

- [ ] **Step 1: Failing tests**
  - `BassFilter` over a buffer gives exactly the samples `bass_boost` produced before normalization (compare against an inline copy of the old loop on a short sine, or assert `bass_boost` output is unchanged on a fixed input by snapshotting a few samples before refactoring).
  - `probe_secs` on the 1 s test wav (`crate::test_util::write_test_wav`) is within 0.01 of the real length; on a missing file is None.
  - The stream source (build it through a `fn stream_source(path, trim, bass, gain) -> Result<impl Source<Item=f32>, AudioError>` helper so it is testable without a device): on a test wav, untrimmed yields all samples (converted like `to_f32` of the decode); trim 0.25–0.5 s yields exactly the frames of that range (±1 frame); gain 2.0 never yields |x| ≥ 1.0.
  - Fake backend records `PlayStream`.
- [ ] **Step 2:** run, see failures.
- [ ] **Step 3:** implement.
- [ ] **Step 4:** all verification commands pass, no new warnings.
- [ ] **Step 5: Commit** `feat(audio): stream playback from a file`.

---

### Task 2: Route long sounds to streaming

**Files:** `core/src/audio/engine.rs`, `src-tauri/src/state.rs` (`warm`), `src-tauri/src/commands.rs` (if any caller needs it).

**Interfaces (engine):**
- `pub const STREAM_SECS: f32 = 30.0;`, `pub fn play_secs(full: f32, trim: Option<Trim>) -> f32`.
- Field `full_secs: HashMap<String, f32>`; `pub fn set_full_secs(&mut self, id, secs)`, `pub fn full_secs(&self, id) -> Option<f32>`; `invalidate` removes it; any full decode in `play_file` or `set_editor` records it.
- `pub fn streams(&self, id, trim) -> bool` — full length known and `play_secs > STREAM_SECS`.
- `play_file(id, vol, path, trim, bass)`: if `streams(id, trim)` → fan out `play_stream` exactly like `play_buffer` does for buffers (reuse its target/fallback loop by making the per-target play a closure or enum, not a copy); cache nothing. Else the cache path; on a miss record `full_secs` from the decode; if the play length is then over `STREAM_SECS`, play the decoded buffer this once without caching it.
- `cached_duration(id, trim)`: when `streams(id, trim)` return `Some(play_secs)`; otherwise as now.
- Re-warm / warm (`src-tauri/src/state.rs`): before decoding, if the engine has no length, probe outside the lock (`probe_secs`) and store it; if the sound then streams, skip decoding (nothing to cache). If the probe returns None, decode as today, store the length, and skip inserting when it streams.

- [ ] **Step 1: Failing tests** (engine.rs, FakeBackend):
  - `play_secs` math (untrimmed, trimmed, trim end past the file).
  - Known long sound: `set_full_secs("a", 60.0)` then `play_file` with a non-existent path → one `PlayStream` call per output with the right trim/bass/gain, nothing cached, `cached_duration` = 60.0 (and the trimmed length when trimmed).
  - Long sound trimmed to 10 s → cache path (use a real short wav and `set_full_secs` to the wav's length, trim so play length < 30).
  - Unknown length, decode shows it is long (set `STREAM_SECS`-independent by writing a wav longer than 30 s? Too slow — instead expose `fn stream_threshold` only in tests, or test with a wav of 31 s at a low sample rate, e.g. 1000 Hz, which is tiny): first play decodes, plays, caches nothing, records the length; the second play streams.
  - Gain change via `set_sound_volume` reaches a stream handle (fake backend gain).
  - `invalidate` forgets the length.
- [ ] **Step 2:** run, see failures.
- [ ] **Step 3:** implement (engine, then warm in state.rs).
- [ ] **Step 4:** all verification commands pass, no new warnings.
- [ ] **Step 5: Commit** `perf(audio): stream sounds longer than 30 s instead of caching them`.
