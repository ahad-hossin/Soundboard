# Stream long sounds — Design Spec

Date: 2026-11-16
Status: Approved by user ("stream long sounds"), awaiting implementation

## Problem

After the 16-bit cache (`2026-11-16-ram-cache-design.md`) the app holds about 177 MB of
decoded audio; about 130 MB of it is the user's 5 sounds longer than 30 s.

## Design

A sound whose **play length** (its trim range, or the whole file) is longer than
`STREAM_SECS = 30.0` is never decoded into memory for playback. Each play opens the file and
decodes it on the fly. Shorter sounds keep the cached path, unchanged.

- **Knowing the length.** The engine keeps `full_secs: HashMap<sound id, f32>`, the file's full
  length. It is filled by a cheap probe (`probe_secs(path)`: open a rodio `Decoder` and read
  `total_duration()`, no decoding) during warming, or from any full decode (a play miss or the
  trim editor). When the probe returns None, warming decodes as today and records the length
  from the decode. Play length = `trim.map(|t| t.end.min(full) - t.start).unwrap_or(full)`.
- **Routing.** `Engine::play_file`: if `full_secs` is known and the play length is over
  `STREAM_SECS`, call the new backend method `play_stream` on every output (same fan-out and
  fallback rules as `play_buffer`). Otherwise use the cache path. If the length is unknown, the
  cache path runs and records the length from its decode; when that turns out long, the
  decoded buffer plays this once and is **not** cached.
- **Streaming source.** `play_stream(target, path, trim, bass, gain)` on the audio thread:
  `Decoder::new(BufReader<File>)`, f32 samples, seek to the trim start (`try_seek`, falling
  back to `skip_duration`), `take_duration` of the trim length, then bass boost (streaming, see
  below), then the same gain and soft-limit stage `SharedSource` uses, with the same live
  `SetGain` handling. Two outputs open the file twice. A file that fails to open returns an
  error like `play` does.
- **Bass on a stream.** `bass.rs` gets a stateful per-channel `BassFilter` (shelf biquad +
  `tanh` saturation) that both `bass_boost` (buffer) and the stream use. The buffer path keeps
  its peak normalization. The stream cannot know its peak, so its bass output goes through
  `soft_limit`, which keeps it at most 1.0. High strengths therefore sound slightly
  different on long sounds than on short ones.
- **Durations.** `cached_duration` returns the play length for a streamed sound, so the tile
  sweep bar and the phone still show progress.
- **Edits.** `set_trim` / `set_bass` / re-warm: a sound whose new play length is over 30 s
  needs no warming and has no cache entry. A long sound trimmed below 30 s goes back to the
  cached path through the existing re-warm.
- **Trim editor.** Unchanged: it decodes the whole file into the editor slot while open, so
  the waveform and Play selection work as today.
- `invalidate` forgets `full_secs` for the sound.

## Out of scope

A setting for the threshold, streaming short sounds, closing the window in the tray.

## Expected result

The user's library: about 177 MB of cache drops to about 45 MB. The first sample of a
streamed sound comes after the file opens and the decoder seeks (a few ms at the start; a
deep trim start costs one seek).

## Testing

Core: `play_secs` math; `probe_secs` on the test wav; routing (a known long play length calls
`play_stream` on the fake backend and caches nothing; a short one caches); a long trim moved
below 30 s caches again; unknown length decoded once is not cached when long; `cached_duration`
for streamed sounds; `BassFilter` gives the same samples as the old buffer path before
normalization; the streaming source honors trim start and length on a test wav; gain changes
reach a stream. Measure the installed app after warm-up.
