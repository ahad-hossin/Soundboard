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
    /// Name of the system default output at the last enumeration, so a Default
    /// fallback never opens a second sink on a device that is already playing.
    default_device: Option<String>,
}

fn default_name(devices: &[DeviceInfo]) -> Option<String> {
    devices.iter().find(|d| d.is_default).map(|d| d.name.clone())
}

impl<B: AudioBackend> Engine<B> {
    pub fn new(backend: B, outputs: Outputs, master: f32, cache_bytes: usize) -> Self {
        let default_device = default_name(&backend.devices());
        Self {
            backend,
            cache: BufferCache::new(cache_bytes),
            master: master.clamp(0.0, 1.0),
            outputs,
            active: Vec::new(),
            next_instance: 1,
            default_device,
        }
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.backend.devices()
    }

    /// Re-enumerates devices, remembers which one is the default, and returns the list.
    pub fn refresh_devices(&mut self) -> Vec<DeviceInfo> {
        let devices = self.backend.devices();
        self.default_device = default_name(&devices);
        devices
    }

    pub fn outputs(&self) -> &Outputs {
        &self.outputs
    }

    /// Stops everything, drops device streams, and switches outputs.
    pub fn set_outputs(&mut self, outputs: Outputs) {
        self.stop_all();
        self.backend.reset();
        self.outputs = outputs;
        self.default_device = default_name(&self.backend.devices());
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
        let mut e = engine(Outputs { speakers: Some("Speakers".into()), virtual_mic: Some("Ghost".into()) }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(play_targets(&e), vec![DeviceTarget::Named("Speakers".into())]);
        assert_eq!(out.sinks, 1);
        assert_eq!(out.fallbacks, vec!["Ghost".to_string()]);
    }

    #[test]
    fn both_slots_missing_play_once_on_default() {
        let mut e = engine(Outputs { speakers: Some("Ghost1".into()), virtual_mic: Some("Ghost2".into()) }, 1.0);
        let out = e.play_buffer("a", 1.0, &clip()).unwrap();
        assert_eq!(play_targets(&e), vec![DeviceTarget::Default]);
        assert_eq!(out.sinks, 1);
        assert_eq!(out.fallbacks, vec!["Ghost1".to_string(), "Ghost2".to_string()]);
    }

    #[test]
    fn named_default_after_fallback_is_skipped() {
        let mut e = engine(Outputs { speakers: Some("Ghost".into()), virtual_mic: Some("Speakers".into()) }, 1.0);
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
