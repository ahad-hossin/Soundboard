use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};
use super::bass::bass_boost;
use super::cache::BufferCache;
use super::decode::decode_file;
use super::trim::apply_trim;
use crate::config::{Trim, MAX_MIC_GAIN, MAX_SOUND_VOLUME};

pub const TEST_TONE_ID: &str = "__test_tone__";
/// Hidden sound id of the trim editor's "Play selection"; never listed in `playing()`.
pub const PREVIEW_ID: &str = "__preview__";

/// Ids that are not library sounds and never show as playing.
fn is_hidden(sound_id: &str) -> bool {
    sound_id == TEST_TONE_ID || sound_id == PREVIEW_ID
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outputs {
    pub speakers: Option<String>,
    pub virtual_mic: Option<String>,
    /// Input device mixed into the virtual mic output; never played on the speakers.
    pub microphone: Option<String>,
    /// Linear passthrough gain, 0..=2.
    pub mic_gain: f32,
}

impl Default for Outputs {
    fn default() -> Self {
        Self { speakers: None, virtual_mic: None, microphone: None, mic_gain: 1.0 }
    }
}

fn clamp_mic_gain(g: f32) -> f32 {
    if g.is_finite() {
        g.clamp(0.0, MAX_MIC_GAIN)
    } else {
        1.0
    }
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

    /// The virtual mic, which carries the user's voice and must play exclusively (see
    /// `AudioBackend::set_exclusive_output`).
    pub fn exclusive_target(&self) -> Option<DeviceTarget> {
        self.virtual_mic.as_ref().map(|n| DeviceTarget::Named(n.clone()))
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
    /// Play buffers: each sound's trimmed, bass-boosted slice, exactly what plays.
    cache: BufferCache,
    /// The (trim, bass) each play buffer was made for. Valid only while `cache` holds the id.
    cut_for: HashMap<String, (Option<Trim>, f32)>,
    /// The trim editor's full decode, held only while the editor is open.
    editor: Option<(String, DecodedBuffer)>,
    master: f32,
    outputs: Outputs,
    active: Vec<Active>,
    next_instance: u64,
    /// Name of the system default output at the last enumeration, so a Default
    /// fallback never opens a second sink on a device that is already playing.
    default_device: Option<String>,
    /// Last mic passthrough start failure; cleared by the next successful apply.
    passthrough_error: Option<String>,
    /// (microphone, virtual mic) of the last successful passthrough start; None when stopped or failed.
    passthrough_key: Option<(String, String)>,
}

fn default_name(devices: &[DeviceInfo]) -> Option<String> {
    devices.iter().find(|d| d.is_default).map(|d| d.name.clone())
}

impl<B: AudioBackend> Engine<B> {
    /// Does not start mic passthrough; call `apply_passthrough` once the engine is set up.
    pub fn new(backend: B, mut outputs: Outputs, master: f32, cache_bytes: usize) -> Self {
        let default_device = default_name(&backend.devices());
        outputs.mic_gain = clamp_mic_gain(outputs.mic_gain);
        backend.set_exclusive_output(outputs.exclusive_target());
        Self {
            backend,
            cache: BufferCache::new(cache_bytes),
            cut_for: HashMap::new(),
            editor: None,
            master: master.clamp(0.0, 1.0),
            outputs,
            active: Vec::new(),
            next_instance: 1,
            default_device,
            passthrough_error: None,
            passthrough_key: None,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.backend.devices()
    }

    pub fn input_devices(&self) -> Vec<DeviceInfo> {
        self.backend.input_devices()
    }

    /// Re-enumerates devices, remembers which one is the default, makes sure mic passthrough
    /// runs as configured without interrupting a healthy one (see `ensure_passthrough`),
    /// and returns the output list.
    pub fn refresh_devices(&mut self) -> Vec<DeviceInfo> {
        let devices = self.backend.devices();
        self.default_device = default_name(&devices);
        let _ = self.ensure_passthrough();
        devices
    }

    /// (microphone, virtual mic) when both are configured, i.e. when passthrough should run.
    fn desired_passthrough(&self) -> Option<(String, String)> {
        match (&self.outputs.microphone, &self.outputs.virtual_mic) {
            (Some(mic), Some(virtual_mic)) => Some((mic.clone(), virtual_mic.clone())),
            _ => None,
        }
    }

    /// Forced path: starts (or restarts) mic passthrough when both a microphone and a virtual
    /// mic are configured, otherwise stops it. Returns whether passthrough is now running.
    /// The outcome is also kept for `passthrough_error`.
    pub fn apply_passthrough(&mut self) -> Result<bool, AudioError> {
        self.passthrough_key = None;
        let desired = self.desired_passthrough();
        let result = match &desired {
            Some((mic, virtual_mic)) => self
                .backend
                .start_passthrough(
                    &DeviceTarget::from_name(Some(mic)),
                    &DeviceTarget::Named(virtual_mic.clone()),
                    self.outputs.mic_gain,
                )
                .map(|()| true),
            None => {
                self.backend.stop_passthrough();
                Ok(false)
            }
        };
        self.passthrough_error = result.as_ref().err().map(|e| e.to_string());
        if matches!(result, Ok(true)) {
            self.passthrough_key = desired;
        }
        result
    }

    /// Gentle path: leaves a healthy passthrough on the configured devices alone (no backend
    /// start or stop, so no audible gap), otherwise falls back to `apply_passthrough`.
    pub fn ensure_passthrough(&mut self) -> Result<bool, AudioError> {
        let desired = self.desired_passthrough();
        let healthy = self.passthrough_error.is_none() && desired == self.passthrough_key;
        match desired {
            Some(_) if healthy && self.backend.passthrough_active() => Ok(true),
            None if healthy => Ok(false),
            _ => self.apply_passthrough(),
        }
    }

    /// True once when a passthrough this engine started has stopped on its own (for example its
    /// output stream was dropped after the device vanished). It is then forgotten, so this does
    /// not fire again until a later start succeeds, and the next `ensure_passthrough` restarts it.
    pub fn passthrough_lost(&mut self) -> bool {
        if self.passthrough_key.is_some() && !self.backend.passthrough_active() {
            self.passthrough_key = None;
            return true;
        }
        false
    }

    /// The last passthrough start failure, if the most recent apply failed.
    pub fn passthrough_error(&self) -> Option<String> {
        self.passthrough_error.clone()
    }

    /// Why the virtual mic is playing in shared mode, where Discord screen shares and per-app
    /// recorders (Medal, OBS) also hear it; None while it is exclusive or not open.
    pub fn virtual_mic_warning(&self) -> Option<String> {
        self.backend.exclusive_fallback()
    }

    pub fn set_mic_gain(&mut self, g: f32) {
        let g = clamp_mic_gain(g);
        self.outputs.mic_gain = g;
        self.backend.set_passthrough_gain(g);
    }

    pub fn outputs(&self) -> &Outputs {
        &self.outputs
    }

    /// Stops everything, drops device streams, switches outputs, and restarts mic
    /// passthrough (see `passthrough_error`). The reset always ends a running passthrough.
    pub fn set_outputs(&mut self, mut outputs: Outputs) {
        self.stop_all();
        self.backend.reset();
        self.passthrough_key = None;
        outputs.mic_gain = clamp_mic_gain(outputs.mic_gain);
        self.backend.set_exclusive_output(outputs.exclusive_target());
        self.outputs = outputs;
        self.default_device = default_name(&self.backend.devices());
        let _ = self.apply_passthrough();
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
        let v = v.clamp(0.0, MAX_SOUND_VOLUME);
        let master = self.master;
        let backend = &self.backend;
        for a in self.active.iter_mut().filter(|a| a.sound_id == sound_id) {
            a.sound_volume = v;
            backend.set_gain(a.handle, v * master);
        }
    }

    /// Plays the file, or only its `trim` range. Only the processed slice is cached, tagged with
    /// the trim and bass it was made for; the full decode is dropped unless the editor holds it.
    pub fn play_file(&mut self, sound_id: &str, sound_volume: f32, path: &Path, trim: Option<Trim>, bass: f32) -> Result<PlayOutcome, AudioError> {
        let hit = if self.cached_tag(sound_id) == Some((trim, bass)) { self.cache.get(sound_id) } else { None };
        let buffer = match hit {
            Some(b) => b,
            None => {
                let play = bass_boost(&apply_trim(&self.full_buffer(sound_id, path)?, trim), bass);
                self.cache.insert(sound_id, play.clone());
                self.cut_for.insert(sound_id.to_string(), (trim, bass));
                // Drop tags of buffers the insert evicted.
                let cache = &self.cache;
                self.cut_for.retain(|id, _| cache.contains(id));
                play
            }
        };
        self.play_buffer(sound_id, sound_volume, &buffer)
    }

    /// The editor's full decode when it holds this sound, else decoded now (never cached).
    fn full_buffer(&self, sound_id: &str, path: &Path) -> Result<DecodedBuffer, AudioError> {
        match self.editor_buffer(sound_id) {
            Some(b) => Ok(b),
            None => decode_file(path),
        }
    }

    /// The (trim, bass) of the cached play buffer, if one is cached.
    fn cached_tag(&self, sound_id: &str) -> Option<(Option<Trim>, f32)> {
        self.cut_for.get(sound_id).copied().filter(|_| self.cache.contains(sound_id))
    }

    fn uncache(&mut self, sound_id: &str) {
        self.cache.remove(sound_id);
        self.cut_for.remove(sound_id);
    }

    /// Drops the play buffer when the trim changed; the next play cuts it again.
    pub fn set_trim(&mut self, sound_id: &str, trim: Option<Trim>) {
        if self.cached_tag(sound_id).is_some_and(|(t, _)| t != trim) {
            self.uncache(sound_id);
        }
    }

    /// Drops the play buffer when the bass changed; the next play builds it again.
    pub fn set_bass(&mut self, sound_id: &str, bass: f32) {
        if self.cached_tag(sound_id).is_some_and(|(_, b)| b != bass) {
            self.uncache(sound_id);
        }
    }

    pub fn is_cached(&self, sound_id: &str) -> bool {
        self.cache.contains(sound_id)
    }

    /// Length in seconds of what plays, when the cached play buffer was cut for `trim`.
    pub fn cached_duration(&self, sound_id: &str, trim: Option<Trim>) -> Option<f32> {
        match self.cached_tag(sound_id) {
            Some((t, _)) if t == trim => self.cache.peek(sound_id).map(|b| b.duration_secs()),
            _ => None,
        }
    }

    /// The trim editor's full decode, if the editor slot holds this sound.
    pub fn editor_buffer(&self, sound_id: &str) -> Option<DecodedBuffer> {
        self.editor.as_ref().filter(|(id, _)| id == sound_id).map(|(_, b)| b.clone())
    }

    /// Holds `full` for the open trim editor, replacing whatever the slot held.
    pub fn set_editor(&mut self, sound_id: &str, full: DecodedBuffer) {
        self.editor = Some((sound_id.to_string(), full));
    }

    pub fn close_editor(&mut self) {
        self.editor = None;
    }

    /// Plays `trim` of the sound under the hidden `PREVIEW_ID`, replacing any running preview.
    pub fn preview(&mut self, sound_id: &str, path: &Path, trim: Trim, sound_volume: f32, bass: f32) -> Result<PlayOutcome, AudioError> {
        self.stop_preview();
        let full = self.full_buffer(sound_id, path)?;
        let slice = bass_boost(&apply_trim(&full, Some(trim)), bass);
        self.play_buffer(PREVIEW_ID, sound_volume, &slice)
    }

    pub fn stop_preview(&mut self) {
        self.stop_sound(PREVIEW_ID);
    }

    /// Adds a play buffer built elsewhere (outside any lock) for `tag` = (trim, bass), ahead of
    /// the first play. Never evicts: returns false and drops the buffer when there is no room.
    pub fn insert_cached(&mut self, sound_id: &str, tag: (Option<Trim>, f32), buffer: DecodedBuffer) -> bool {
        if !self.cache.is_empty() && !self.cache.has_room_for(buffer.byte_len()) {
            return false;
        }
        self.cache.insert(sound_id, buffer);
        self.cut_for.insert(sound_id.to_string(), tag);
        true
    }

    /// Plays `buffer` on every output. A sound that is already playing is stopped first, so
    /// replaying restarts it; different sounds overlap.
    pub fn play_buffer(&mut self, sound_id: &str, sound_volume: f32, buffer: &DecodedBuffer) -> Result<PlayOutcome, AudioError> {
        let targets = self.outputs.targets();
        if targets.is_empty() {
            return Err(AudioError::NoOutputs);
        }
        if sound_id != TEST_TONE_ID {
            self.stop_sound(sound_id);
        }
        let sound_volume = sound_volume.clamp(0.0, MAX_SOUND_VOLUME);
        let gain = sound_volume * self.master;
        let instance = self.next_instance;
        self.next_instance += 1;

        let mut fallbacks = Vec::new();
        let mut sinks = 0;
        let mut last_err = None;
        // True once a sink is open on the default device, whether it was reached by
        // a Default fallback or by naming it. Prevents doubled playback on one device.
        let mut played_default = false;
        for target in targets {
            let is_default_name = matches!(&target, DeviceTarget::Named(n) if Some(n) == self.default_device.as_ref());
            if is_default_name && played_default {
                continue;
            }
            let result = match self.backend.play(&target, buffer, gain) {
                Err(AudioError::DeviceNotFound(name)) => {
                    fallbacks.push(name);
                    if played_default {
                        continue;
                    }
                    let r = self.backend.play(&DeviceTarget::Default, buffer, gain);
                    if r.is_ok() {
                        played_default = true;
                    }
                    r
                }
                Ok(h) => {
                    if is_default_name {
                        played_default = true;
                    }
                    Ok(h)
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

    /// Stops every active sink of one sound.
    fn stop_sound(&mut self, sound_id: &str) {
        let backend = &self.backend;
        self.active.retain(|a| {
            if a.sound_id == sound_id {
                backend.stop(a.handle);
                false
            } else {
                true
            }
        });
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

    /// Unique sound ids with at least one active sink, in start order. Test tones and the trim
    /// preview excluded.
    pub fn playing(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        self.active
            .iter()
            .filter(|a| !is_hidden(&a.sound_id))
            .filter(|a| seen.insert(a.sound_id.clone()))
            .map(|a| a.sound_id.clone())
            .collect()
    }

    pub fn invalidate(&mut self, sound_id: &str) {
        self.uncache(sound_id);
        if self.editor_buffer(sound_id).is_some() {
            self.close_editor();
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fake_backend::{Call, FakeBackend};
    use crate::test_util::write_test_wav;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn both() -> Outputs {
        Outputs { speakers: Some("Speakers".into()), virtual_mic: Some("CABLE Input".into()), ..Outputs::default() }
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
        let mut e = engine(Outputs { speakers: Some("Ghost".into()), virtual_mic: None, ..Outputs::default() }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(out.sinks, 1);
        assert_eq!(out.fallbacks, vec!["Ghost".to_string()]);
        assert!(matches!(e.backend().calls()[0], Call::Play { target: DeviceTarget::Default, .. }));
    }

    fn play_targets(e: &Engine<FakeBackend>) -> Vec<DeviceTarget> {
        e.backend()
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Play { target, .. } => Some(target),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn fallback_skipped_when_other_slot_is_the_default_device() {
        let mut e = engine(Outputs { speakers: Some("Speakers".into()), virtual_mic: Some("Ghost".into()), ..Outputs::default() }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(play_targets(&e), vec![DeviceTarget::Named("Speakers".into())]);
        assert_eq!(out.sinks, 1);
        assert_eq!(out.fallbacks, vec!["Ghost".to_string()]);
    }

    #[test]
    fn sound_volume_goes_past_one_but_not_past_max() {
        let mut e = engine(both(), 0.5);
        e.play_buffer("a", 1.8, &clip()).unwrap();
        let h = e.active[0].handle;
        assert!((e.backend().gain(h).unwrap() - 0.9).abs() < 1e-6);
        e.set_sound_volume("a", 5.0);
        assert!((e.backend().gain(h).unwrap() - 1.0).abs() < 1e-6, "clamped to 2.0 x 0.5");
    }

    #[test]
    fn both_slots_missing_play_once_on_default() {
        let mut e = engine(Outputs { speakers: Some("Ghost1".into()), virtual_mic: Some("Ghost2".into()), ..Outputs::default() }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(play_targets(&e), vec![DeviceTarget::Default]);
        assert_eq!(out.sinks, 1);
        assert_eq!(out.fallbacks, vec!["Ghost1".to_string(), "Ghost2".to_string()]);
    }

    #[test]
    fn named_default_after_fallback_is_skipped() {
        let mut e = engine(Outputs { speakers: Some("Ghost".into()), virtual_mic: Some("Speakers".into()), ..Outputs::default() }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(play_targets(&e), vec![DeviceTarget::Default]);
        assert_eq!(out.fallbacks, vec!["Ghost".to_string()]);
    }

    #[test]
    fn refresh_devices_returns_list() {
        let mut e = engine(both(), 1.0);
        let d = e.refresh_devices();
        assert_eq!(d.len(), 2);
        assert!(d[0].is_default);
    }

    #[test]
    fn no_outputs_is_error() {
        let mut e = engine(Outputs::default(), 1.0);
        assert!(matches!(e.play_buffer("a", 1.0, &clip()), Err(AudioError::NoOutputs)));
    }

    #[test]
    fn same_device_in_both_slots_plays_once() {
        let mut e = engine(Outputs { speakers: Some("Speakers".into()), virtual_mic: Some("Speakers".into()), ..Outputs::default() }, 1.0);
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
        let mut e = engine(Outputs { speakers: Some("Speakers".into()), virtual_mic: None, ..Outputs::default() }, 1.0);
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
        e.play_file("a", 1.0, &p, None, 0.0).unwrap();
        e.play_file("a", 1.0, &p, None, 0.0).unwrap();
        assert_eq!(e.cached(), 1);
        e.invalidate("a");
        assert_eq!(e.cached(), 0);
    }

    #[test]
    fn insert_cached_fills_cache_without_playing_and_play_uses_it() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 500);
        let mut e = engine(both(), 1.0);
        assert!(!e.is_cached("a"));
        assert!(e.insert_cached("a", (None, 0.0), decode_file(&p).unwrap()));
        assert!(e.is_cached("a"));
        assert_eq!(e.cached(), 1);
        assert!(e.backend().calls().is_empty(), "no Play call");
        // Path no longer needed: play hits the cache instead of decoding.
        e.play_file("a", 1.0, Path::new("Z:/gone.wav"), None, 0.0).unwrap();
        assert_eq!(e.cached(), 1);
    }

    #[test]
    fn cached_duration_is_none_before_and_some_after_insert() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 500);
        let mut e = engine(both(), 1.0);
        assert_eq!(e.cached_duration("a", None), None);
        let buf = decode_file(&p).unwrap();
        let expected = buf.duration_secs();
        assert!(e.insert_cached("a", (None, 0.0), buf));
        assert_eq!(e.cached_duration("a", None), Some(expected));
        e.invalidate("a");
        assert_eq!(e.cached_duration("a", None), None);
    }

    #[test]
    fn insert_cached_never_evicts() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 500);
        let buf = decode_file(&p).unwrap();
        // Room for exactly one decoded clip.
        let mut e = Engine::new(FakeBackend::new(&["Speakers"]), both(), 1.0, buf.byte_len());
        assert!(e.insert_cached("a", (None, 0.0), buf.clone()));
        assert!(!e.insert_cached("b", (None, 0.0), buf));
        assert!(e.is_cached("a"));
        assert!(!e.is_cached("b"));
    }

    #[test]
    fn play_file_missing_is_io_error() {
        let mut e = engine(both(), 1.0);
        assert!(matches!(e.play_file("a", 1.0, Path::new("Z:/nope.wav"), None, 0.0), Err(AudioError::Io(_))));
    }

    #[test]
    fn new_plays_the_virtual_mic_exclusively() {
        let e = engine(both(), 1.0);
        assert_eq!(e.backend().exclusive_output(), Some(DeviceTarget::Named("CABLE Input".into())));
        let e = engine(Outputs { speakers: Some("Speakers".into()), ..Outputs::default() }, 1.0);
        assert_eq!(e.backend().exclusive_output(), None);
    }

    #[test]
    fn set_outputs_moves_the_exclusive_output_with_the_virtual_mic() {
        let mut e = engine(both(), 1.0);
        e.set_outputs(Outputs { speakers: Some("Speakers".into()), ..Outputs::default() });
        assert_eq!(e.backend().exclusive_output(), None);
        e.set_outputs(Outputs { speakers: Some("CABLE Input".into()), virtual_mic: Some("Speakers".into()), ..Outputs::default() });
        assert_eq!(e.backend().exclusive_output(), Some(DeviceTarget::Named("Speakers".into())));
    }

    #[test]
    fn virtual_mic_warning_reports_the_shared_mode_fallback() {
        let e = engine(both(), 1.0);
        assert_eq!(e.virtual_mic_warning(), None);
        e.backend().set_exclusive_fallback(Some("the device is in use by another app".into()));
        assert_eq!(e.virtual_mic_warning().as_deref(), Some("the device is in use by another app"));
    }

    #[test]
    fn set_outputs_resets_backend_and_stops_sound() {
        let mut e = engine(both(), 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        e.set_outputs(Outputs { speakers: Some("Speakers".into()), virtual_mic: None, ..Outputs::default() });
        assert!(e.playing().is_empty());
        assert!(e.backend().calls().contains(&Call::Reset));
        assert_eq!(e.outputs().targets().len(), 1);
    }

    #[test]
    fn replaying_a_sound_restarts_it() {
        let mut e = engine(both(), 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        let first = e.backend().handles();
        assert_eq!(first.len(), 2);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        let calls = e.backend().calls();
        for h in &first {
            assert!(calls.contains(&Call::Stop(*h)), "first play's sink {h:?} stopped");
        }
        assert_eq!(calls.iter().filter(|c| matches!(c, Call::Play { .. })).count(), 4);
        assert_eq!(calls.iter().filter(|c| matches!(c, Call::Stop(_))).count(), 2);
        assert_eq!(e.playing(), vec!["a".to_string()]);
        e.stop_all();
        let stops = e.backend().calls().iter().filter(|c| matches!(c, Call::Stop(_))).count();
        assert_eq!(stops, 4, "only the second play's sinks were still active");
    }

    #[test]
    fn different_sounds_still_overlap() {
        let mut e = engine(both(), 1.0);
        e.play_buffer("a", 1.0, &clip()).unwrap();
        e.play_buffer("b", 1.0, &clip()).unwrap();
        assert!(!e.backend().calls().iter().any(|c| matches!(c, Call::Stop(_))));
        assert_eq!(e.playing(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_tones_do_not_cut_each_other() {
        let mut e = engine(both(), 1.0);
        let speakers = DeviceTarget::Named("Speakers".into());
        e.test_tone(&speakers).unwrap();
        e.test_tone(&speakers).unwrap();
        e.play_buffer(TEST_TONE_ID, 1.0, &clip()).unwrap();
        assert!(!e.backend().calls().iter().any(|c| matches!(c, Call::Stop(_))));
    }

    #[test]
    fn test_tone_not_listed_in_playing() {
        let mut e = engine(both(), 1.0);
        e.test_tone(&DeviceTarget::Named("Speakers".into())).unwrap();
        assert!(e.playing().is_empty());
        assert_eq!(e.backend().handles().len(), 1);
    }

    fn mic_engine(outputs: Outputs) -> Engine<FakeBackend> {
        Engine::new(FakeBackend::with_inputs(&["Speakers", "CABLE Input"], &["Headset Mic"]), outputs, 1.0, 1024 * 1024)
    }

    fn with_mic(mic: Option<&str>, virtual_mic: Option<&str>) -> Outputs {
        Outputs {
            speakers: Some("Speakers".into()),
            virtual_mic: virtual_mic.map(String::from),
            microphone: mic.map(String::from),
            mic_gain: 1.25,
        }
    }

    fn passthrough_calls(e: &Engine<FakeBackend>) -> Vec<Call> {
        e.backend()
            .calls()
            .into_iter()
            .filter(|c| matches!(c, Call::StartPassthrough { .. } | Call::StopPassthrough | Call::SetPassthroughGain(_)))
            .collect()
    }

    #[test]
    fn outputs_default_mic_gain_is_one() {
        assert_eq!(Outputs::default().mic_gain, 1.0);
        assert_eq!(Outputs::default().microphone, None);
    }

    #[test]
    fn new_does_not_start_passthrough() {
        let e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        assert!(passthrough_calls(&e).is_empty());
        assert_eq!(e.input_devices().len(), 1);
        assert!(e.input_devices()[0].is_default);
    }

    #[test]
    fn set_outputs_with_mic_and_virtual_mic_starts_passthrough() {
        let mut e = mic_engine(Outputs::default());
        e.set_outputs(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        assert_eq!(
            passthrough_calls(&e),
            vec![Call::StartPassthrough {
                input: DeviceTarget::Named("Headset Mic".into()),
                output: DeviceTarget::Named("CABLE Input".into()),
                gain: 1.25,
            }]
        );
        assert_eq!(e.passthrough_error(), None);
    }

    #[test]
    fn set_outputs_without_mic_stops_passthrough() {
        let mut e = mic_engine(Outputs::default());
        e.set_outputs(with_mic(None, Some("CABLE Input")));
        assert_eq!(passthrough_calls(&e), vec![Call::StopPassthrough]);
    }

    #[test]
    fn set_outputs_without_virtual_mic_stops_and_never_starts() {
        let mut e = mic_engine(Outputs::default());
        e.set_outputs(with_mic(Some("Headset Mic"), None));
        assert_eq!(passthrough_calls(&e), vec![Call::StopPassthrough]);
    }

    #[test]
    fn apply_passthrough_reports_whether_it_runs() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        assert!(e.apply_passthrough().unwrap());
        let mut e = mic_engine(with_mic(None, Some("CABLE Input")));
        assert!(!e.apply_passthrough().unwrap());
    }

    #[test]
    fn unknown_mic_sets_passthrough_error_until_a_successful_apply() {
        let mut e = mic_engine(Outputs::default());
        e.set_outputs(with_mic(Some("Ghost Mic"), Some("CABLE Input")));
        let err = e.passthrough_error().expect("error recorded");
        assert!(err.contains("microphone not found"), "{err}");
        e.set_outputs(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        assert_eq!(e.passthrough_error(), None);
    }

    fn starts(e: &Engine<FakeBackend>) -> usize {
        passthrough_calls(e).into_iter().filter(|c| matches!(c, Call::StartPassthrough { .. })).count()
    }

    #[test]
    fn refresh_devices_leaves_a_healthy_passthrough_alone() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        e.refresh_devices();
        e.refresh_devices();
        assert_eq!(starts(&e), 1, "second refresh must not reopen the mic");
        assert_eq!(passthrough_calls(&e).len(), 1, "and must not stop it either");
        e.set_mic_gain(0.5);
        e.refresh_devices();
        assert_eq!(starts(&e), 1, "a gain change never restarts");
    }

    #[test]
    fn refresh_after_a_failed_start_retries() {
        let mut e = mic_engine(Outputs::default());
        e.set_outputs(with_mic(Some("USB Mic"), Some("CABLE Input")));
        assert!(e.passthrough_error().unwrap().contains("microphone not found"));
        e.backend().plug_input("USB Mic");
        e.refresh_devices();
        assert_eq!(starts(&e), 1, "the failed start was retried");
        assert_eq!(e.passthrough_error(), None);
    }

    #[test]
    fn refresh_restarts_when_the_backend_reports_inactive() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        e.refresh_devices();
        e.backend().kill_passthrough();
        e.refresh_devices();
        assert_eq!(starts(&e), 2);
        assert!(e.backend().passthrough_active());
    }

    #[test]
    fn passthrough_lost_fires_once_and_ensure_restarts() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        assert!(!e.passthrough_lost(), "never started");
        e.apply_passthrough().unwrap();
        assert!(!e.passthrough_lost(), "still running");
        e.backend().kill_passthrough();
        assert!(e.passthrough_lost());
        assert!(!e.passthrough_lost(), "reported once");
        assert!(e.ensure_passthrough().unwrap());
        assert_eq!(starts(&e), 2);
        assert!(!e.passthrough_lost());
    }

    #[test]
    fn passthrough_lost_is_silent_after_a_deliberate_stop() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        e.apply_passthrough().unwrap();
        e.set_outputs(with_mic(None, Some("CABLE Input")));
        assert!(!e.passthrough_lost());
    }

    #[test]
    fn refresh_without_mic_does_not_touch_the_backend_twice() {
        let mut e = mic_engine(with_mic(None, Some("CABLE Input")));
        e.refresh_devices();
        assert!(passthrough_calls(&e).is_empty(), "nothing started, nothing to stop");
        e.set_outputs(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        e.outputs.microphone = None;
        e.refresh_devices();
        assert_eq!(passthrough_calls(&e).last(), Some(&Call::StopPassthrough), "a dropped mic is stopped");
    }

    #[test]
    fn set_mic_gain_forwards_and_clamps() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        e.set_mic_gain(1.5);
        assert_eq!(e.outputs().mic_gain, 1.5);
        e.set_mic_gain(3.0);
        assert_eq!(e.outputs().mic_gain, 2.0);
        e.set_mic_gain(-1.0);
        assert_eq!(e.outputs().mic_gain, 0.0);
        assert_eq!(
            passthrough_calls(&e),
            vec![Call::SetPassthroughGain(1.5), Call::SetPassthroughGain(2.0), Call::SetPassthroughGain(0.0)]
        );
    }

    #[test]
    fn mic_is_never_a_playback_target() {
        let mut e = mic_engine(with_mic(Some("Headset Mic"), Some("CABLE Input")));
        assert_eq!(
            e.outputs().targets(),
            vec![DeviceTarget::Named("Speakers".into()), DeviceTarget::Named("CABLE Input".into())]
        );
        e.play_buffer("a", 1.0, &clip()).unwrap();
        assert!(!play_targets(&e).contains(&DeviceTarget::Named("Headset Mic".into())));
    }

    #[test]
    fn sine_has_expected_length() {
        let s = sine(440.0, 0.4, 44100);
        assert_eq!(s.samples.len(), 17640);
        assert_eq!(s.channels, 1);
    }

    fn trim(start_secs: f32, end_secs: f32) -> Option<Trim> {
        Some(Trim { start_secs, end_secs })
    }

    fn played_lengths(e: &Engine<FakeBackend>) -> Vec<usize> {
        e.backend()
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Play { samples, .. } => Some(samples),
                _ => None,
            })
            .collect()
    }

    fn speakers_only() -> Outputs {
        Outputs { speakers: Some("Speakers".into()), ..Outputs::default() }
    }

    #[test]
    fn play_file_caches_only_the_trimmed_slice() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 2 * 44100); // 2 s stereo
        let mut e = engine(speakers_only(), 1.0);
        e.play_file("a", 1.0, &p, trim(0.0, 0.5), 0.0).unwrap();
        assert_eq!(played_lengths(&e), vec![2 * 22050]);
        let slice = apply_trim(&decode_file(&p).unwrap(), trim(0.0, 0.5));
        assert_eq!(e.cache.total_bytes(), slice.byte_len(), "cache holds the slice, not the whole file");
        assert!((e.cached_duration("a", trim(0.0, 0.5)).unwrap() - 0.5).abs() < 1e-4);
        assert_eq!(e.cached_duration("a", None), None, "another trim has no cached length");
        assert_eq!(e.cached_duration("a", trim(0.1, 0.5)), None);
    }

    #[test]
    fn play_buffer_is_reused_until_the_trim_changes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 44100);
        let mut e = engine(speakers_only(), 1.0);
        e.play_file("a", 1.0, &p, trim(0.0, 0.5), 0.0).unwrap();
        let first = e.cache.peek("a").unwrap().samples.clone();
        e.play_file("a", 1.0, &p, trim(0.0, 0.5), 0.0).unwrap();
        assert!(Arc::ptr_eq(&first, &e.cache.peek("a").unwrap().samples), "no second slice");
        e.set_trim("a", trim(0.0, 0.5));
        assert!(e.is_cached("a"), "same trim keeps the slice");
        e.set_trim("a", trim(0.1, 0.5));
        assert!(!e.is_cached("a"), "changed trim drops it");
        e.play_file("a", 1.0, &p, trim(0.1, 0.5), 0.0).unwrap();
        let second = e.cache.peek("a").unwrap().samples.clone();
        e.play_file("a", 1.0, &p, None, 0.0).unwrap();
        assert!(!Arc::ptr_eq(&second, &e.cache.peek("a").unwrap().samples), "a different trim rebuilds it");
        assert_eq!(e.cache.total_bytes(), 2 * 44100 * 2, "only the untrimmed buffer is left");
        assert_eq!(played_lengths(&e), vec![2 * 22050, 2 * 22050, 2 * 17640, 2 * 44100]);
    }

    #[test]
    fn play_buffer_is_reused_until_the_bass_changes() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 44100);
        let mut e = engine(speakers_only(), 1.0);
        e.play_file("a", 1.0, &p, None, 0.5).unwrap();
        let first = e.cache.peek("a").unwrap().samples.clone();
        e.play_file("a", 1.0, &p, None, 0.5).unwrap();
        assert!(Arc::ptr_eq(&first, &e.cache.peek("a").unwrap().samples), "no second pass");
        e.set_bass("a", 0.5);
        assert!(e.is_cached("a"), "same bass keeps it");
        e.set_bass("a", 0.8);
        assert!(!e.is_cached("a"), "changed bass drops it");
        e.play_file("a", 1.0, &p, None, 0.8).unwrap();
        let boosted = e.cache.peek("a").unwrap().samples.clone();
        e.play_file("a", 1.0, &p, None, 0.0).unwrap();
        assert!(!Arc::ptr_eq(&boosted, &e.cache.peek("a").unwrap().samples), "plain play rebuilds without the boost");
        assert_eq!(e.cached(), 1, "one buffer per sound");
    }

    #[test]
    fn invalidate_clears_the_play_buffer_and_its_tag() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 4410);
        let mut e = engine(speakers_only(), 1.0);
        e.play_file("a", 1.0, &p, trim(0.0, 0.06), 0.0).unwrap();
        e.invalidate("a");
        assert!(!e.is_cached("a"));
        assert!(e.cut_for.is_empty());
        assert_eq!(e.cached_duration("a", trim(0.0, 0.06)), None);
    }

    #[test]
    fn evicted_buffers_drop_their_tags() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 4410);
        let room_for_one = decode_file(&p).unwrap().byte_len();
        let mut e = Engine::new(FakeBackend::new(&["Speakers"]), speakers_only(), 1.0, room_for_one);
        e.play_file("a", 1.0, &p, None, 0.0).unwrap();
        e.play_file("b", 1.0, &p, None, 0.0).unwrap();
        assert!(!e.is_cached("a"));
        assert_eq!(e.cut_for.keys().collect::<Vec<_>>(), vec!["b"]);
    }

    #[test]
    fn editor_slot_feeds_preview_and_play_without_decoding() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 44100);
        let full = decode_file(&p).unwrap();
        let gone = Path::new("Z:/gone.wav");
        let mut e = engine(speakers_only(), 1.0);
        assert!(e.editor_buffer("a").is_none());
        e.set_editor("a", full.clone());
        assert!(Arc::ptr_eq(&e.editor_buffer("a").unwrap().samples, &full.samples));
        assert!(e.editor_buffer("b").is_none(), "the slot holds one sound");
        e.preview("a", gone, Trim { start_secs: 0.0, end_secs: 0.5 }, 1.0, 0.0).unwrap();
        e.play_file("a", 1.0, gone, trim(0.5, 1.0), 0.0).unwrap();
        assert_eq!(played_lengths(&e), vec![2 * 22050, 2 * 22050]);
        assert!(matches!(e.play_file("b", 1.0, gone, None, 0.0), Err(AudioError::Io(_))), "other sounds still decode");
        e.close_editor();
        assert!(e.editor_buffer("a").is_none(), "close_editor frees the slot");
        e.set_editor("a", full);
        e.invalidate("a");
        assert!(e.editor_buffer("a").is_none(), "invalidate frees the slot too");
        assert!(!e.is_cached("a"));
    }

    #[test]
    fn warmed_buffer_plays_without_decoding_only_for_its_tag() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 44100);
        let play = bass_boost(&apply_trim(&decode_file(&p).unwrap(), trim(0.0, 0.5)), 0.5);
        let gone = Path::new("Z:/gone.wav");
        let mut e = engine(speakers_only(), 1.0);
        assert!(e.insert_cached("a", (trim(0.0, 0.5), 0.5), play.clone()));
        e.play_file("a", 1.0, gone, trim(0.0, 0.5), 0.5).unwrap();
        assert!(Arc::ptr_eq(&play.samples, &e.cache.peek("a").unwrap().samples));
        assert!(matches!(e.play_file("a", 1.0, gone, trim(0.0, 0.5), 0.0), Err(AudioError::Io(_))), "another tag decodes");
    }

    #[test]
    fn preview_plays_hidden_and_replaces_the_previous_preview() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 44100);
        let mut e = engine(speakers_only(), 0.5);
        let t = Trim { start_secs: 0.5, end_secs: 1.0 };
        e.preview("a", &p, t, 0.8, 0.0).unwrap();
        assert!(e.playing().is_empty(), "preview is not a playing sound");
        assert!(!e.is_cached("a"), "preview caches nothing");
        let first = e.backend().handles();
        e.play_file("a", 1.0, &p, None, 0.0).unwrap();
        e.preview("a", &p, t, 0.8, 0.0).unwrap();
        let calls = e.backend().calls();
        assert!(calls.contains(&Call::Stop(first[0])), "second preview stops the first");
        assert_eq!(e.playing(), vec!["a".to_string()], "the real play keeps going");
        assert_eq!(played_lengths(&e), vec![2 * 22050, 2 * 44100, 2 * 22050]);
        e.stop_preview();
        assert_eq!(e.playing(), vec!["a".to_string()]);
        assert_eq!(e.backend().calls().iter().filter(|c| matches!(c, Call::Stop(_))).count(), 2);
    }
}
