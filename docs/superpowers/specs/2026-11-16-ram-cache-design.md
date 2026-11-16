# Less RAM for the decode cache — Design Spec

Date: 2026-11-16
Status: Approved by user ("only cache fixes"), awaiting implementation

## Problem

The app process uses about 520 MB of private memory. Almost all of it is the decode cache:
every sound is decoded to interleaved `f32` PCM and kept in memory, up to `CACHE_BYTES`
(raised to 400 MB for 10-minute link sounds). The user's 20 sounds decode to 353 MB of `f32`.
On top of that the cache holds the *whole* file even when a sound is trimmed, plus a second
processed copy when a sound is trimmed or bass-boosted.

## Fixes (in scope)

1. **16-bit samples.** `DecodedBuffer.samples` becomes `Arc<Vec<i16>>`. rodio's decoder already
   yields `i16`, so decoding stores it as is; playback converts each sample to `f32`
   (`s as f32 / 32768.0`) in `SharedSource` before the gain and limiter. Bass boost and the
   waveform convert to `f32` internally and the bass output converts back (clamped, rounded).
   Halves every cached sound (user's library: 353 MB → 177 MB). No audible loss: the sources
   are 16-bit mp3/wav/ogg/flac decodes.
2. **Cache only what plays.** The LRU cache holds, per sound, the buffer that actually plays:
   the trimmed and bass-boosted slice, or the whole decode when neither applies, tagged with the
   `(trim, bass)` it was built for. The full decode is not kept after the play buffer is cut.
   - The trim editor needs the whole file. The engine keeps one **editor slot**
     (`Option<(sound id, full buffer)>`) outside the cache budget. `get_waveform`, `set_trim`
     validation and `preview_trim` use it. The editor frees it on close through a new command
     `close_trim_editor` (called from the TrimEditor's unmount cleanup).
   - `set_trim` / `set_bass` drop the cached play buffer when its tag no longer matches, then
     re-warm that sound in the background (`spawn_warm(Some(vec![id]))`), so the next press
     is instant again.
   - Startup warming (`spawn_warm`) builds the play buffer (trim + bass) outside the lock and
     inserts it with its tag; it still never evicts.
   - `UiSound.duration_secs` is the cached play buffer's length when its trim tag matches the
     sound's trim; otherwise None (as today when not cached).
3. **Cache budget back to 200 MB.** With 16-bit samples a 10-minute 48 kHz stereo link sound is
   about 110 MB, so it fits next to normal sounds; a trimmed one is tiny.

## Out of scope

Closing the WebView while in the tray; streaming long sounds from disk; a cache size setting.

## Compatibility

No config change. Audio output at ≤100% gain changes only by 16-bit quantization of the decode
(the decoder's native format), which is inaudible.

## Testing

Core unit tests: i16 round trip and conversion in `DecodedBuffer::new` (f32 → i16, clamped);
`byte_len` counts 2 bytes per sample; SharedSource converts and applies gain; trim/waveform/bass
on i16; engine caches the trimmed slice only (cached byte size equals the slice, not the file);
tag mismatch drops the entry; editor slot used by preview and freed by `close_editor`;
`cached_duration` follows the trim tag. UI: TrimEditor calls `closeTrimEditor` on unmount.
Measure the installed app's private memory before and after.
