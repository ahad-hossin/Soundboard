use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};
use crate::config::Trim;

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Play { handle: SinkHandle, target: DeviceTarget, gain: f32, samples: usize },
    PlayStream { handle: SinkHandle, target: DeviceTarget, path: PathBuf, trim: Option<Trim>, bass: f32, gain: f32 },
    Stop(SinkHandle),
    SetGain(SinkHandle, f32),
    Reset,
    StartPassthrough { input: DeviceTarget, output: DeviceTarget, gain: f32 },
    StopPassthrough,
    SetPassthroughGain(f32),
}

/// Records every call; the first device name given (output or input) is the default device.
pub struct FakeBackend {
    devices: Vec<DeviceInfo>,
    inputs: Mutex<Vec<DeviceInfo>>,
    next: AtomicU64,
    calls: Mutex<Vec<Call>>,
    finished: Mutex<HashSet<SinkHandle>>,
    gains: Mutex<HashMap<SinkHandle, f32>>,
    passthrough: AtomicBool,
    /// Kept out of `calls` so call-order assertions stay about playback.
    exclusive: Mutex<Option<DeviceTarget>>,
    exclusive_fallback: Mutex<Option<String>>,
}

fn infos(names: &[&str]) -> Vec<DeviceInfo> {
    names.iter().enumerate().map(|(i, n)| DeviceInfo { name: n.to_string(), is_default: i == 0 }).collect()
}

impl FakeBackend {
    /// Output devices only; the input list is empty.
    pub fn new(device_names: &[&str]) -> Self {
        Self::with_inputs(device_names, &[])
    }

    pub fn with_inputs(outputs: &[&str], inputs: &[&str]) -> Self {
        Self {
            devices: infos(outputs),
            inputs: Mutex::new(infos(inputs)),
            next: AtomicU64::new(1),
            calls: Mutex::new(Vec::new()),
            finished: Mutex::new(HashSet::new()),
            gains: Mutex::new(HashMap::new()),
            passthrough: AtomicBool::new(false),
            exclusive: Mutex::new(None),
            exclusive_fallback: Mutex::new(None),
        }
    }

    /// The target last passed to `set_exclusive_output`.
    pub fn exclusive_output(&self) -> Option<DeviceTarget> {
        self.exclusive.lock().unwrap().clone()
    }

    /// Simulates the exclusive output falling back to shared mode (or recovering with None).
    pub fn set_exclusive_fallback(&self, reason: Option<String>) {
        *self.exclusive_fallback.lock().unwrap() = reason;
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// Handles in play order, buffered and streamed.
    pub fn handles(&self) -> Vec<SinkHandle> {
        self.calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Play { handle, .. } | Call::PlayStream { handle, .. } => Some(handle),
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

    /// Simulates plugging in a microphone after construction.
    pub fn plug_input(&self, name: &str) {
        self.inputs.lock().unwrap().push(DeviceInfo { name: name.to_string(), is_default: false });
    }

    /// Checks `target` like a real device lookup, then records `call` under a new handle.
    fn record_play(&self, target: &DeviceTarget, gain: f32, call: impl FnOnce(SinkHandle) -> Call) -> Result<SinkHandle, AudioError> {
        if let DeviceTarget::Named(n) = target {
            if !self.devices.iter().any(|d| &d.name == n) {
                return Err(AudioError::DeviceNotFound(n.clone()));
            }
        }
        let handle = SinkHandle(self.next.fetch_add(1, Ordering::SeqCst));
        self.calls.lock().unwrap().push(call(handle));
        self.gains.lock().unwrap().insert(handle, gain);
        Ok(handle)
    }

    /// Simulates the passthrough dying on its own (output stream dropped).
    pub fn kill_passthrough(&self) {
        self.passthrough.store(false, Ordering::SeqCst);
    }
}

impl AudioBackend for FakeBackend {
    fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.clone()
    }

    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        self.record_play(target, gain, |handle| Call::Play {
            handle,
            target: target.clone(),
            gain,
            samples: buffer.samples.len(),
        })
    }

    fn play_stream(&self, target: &DeviceTarget, path: &Path, trim: Option<Trim>, bass: f32, gain: f32) -> Result<SinkHandle, AudioError> {
        self.record_play(target, gain, |handle| Call::PlayStream {
            handle,
            target: target.clone(),
            path: path.to_path_buf(),
            trim,
            bass,
            gain,
        })
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
        self.passthrough.store(false, Ordering::SeqCst);
    }

    fn input_devices(&self) -> Vec<DeviceInfo> {
        self.inputs.lock().unwrap().clone()
    }

    fn start_passthrough(&self, input: &DeviceTarget, output: &DeviceTarget, gain: f32) -> Result<(), AudioError> {
        if let DeviceTarget::Named(n) = input {
            if !self.inputs.lock().unwrap().iter().any(|d| &d.name == n) {
                return Err(AudioError::InputDeviceNotFound(n.clone()));
            }
        }
        if let DeviceTarget::Named(n) = output {
            if !self.devices.iter().any(|d| &d.name == n) {
                return Err(AudioError::DeviceNotFound(n.clone()));
            }
        }
        self.calls.lock().unwrap().push(Call::StartPassthrough { input: input.clone(), output: output.clone(), gain });
        self.passthrough.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop_passthrough(&self) {
        self.calls.lock().unwrap().push(Call::StopPassthrough);
        self.passthrough.store(false, Ordering::SeqCst);
    }

    fn set_passthrough_gain(&self, gain: f32) {
        self.calls.lock().unwrap().push(Call::SetPassthroughGain(gain));
    }

    fn passthrough_active(&self) -> bool {
        self.passthrough.load(Ordering::SeqCst)
    }

    fn set_exclusive_output(&self, target: Option<DeviceTarget>) {
        *self.exclusive.lock().unwrap() = target;
    }

    fn exclusive_fallback(&self) -> Option<String> {
        self.exclusive_fallback.lock().unwrap().clone()
    }
}

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
        assert!(f.input_devices().is_empty(), "new() has no inputs");
    }

    #[test]
    fn fake_records_streams_and_their_handles() {
        let f = FakeBackend::new(&["Speakers"]);
        let trim = Some(Trim { start_secs: 1.0, end_secs: 40.0 });
        let h = f.play_stream(&DeviceTarget::Default, Path::new("long.mp3"), trim, 0.5, 0.75).unwrap();
        assert_eq!(
            f.calls(),
            vec![Call::PlayStream {
                handle: h,
                target: DeviceTarget::Default,
                path: PathBuf::from("long.mp3"),
                trim,
                bass: 0.5,
                gain: 0.75,
            }]
        );
        assert_eq!(f.handles(), vec![h]);
        assert_eq!(f.gain(h), Some(0.75));
        let err = f.play_stream(&DeviceTarget::Named("Ghost".into()), Path::new("long.mp3"), None, 0.0, 1.0);
        assert!(matches!(err, Err(AudioError::DeviceNotFound(_))));
    }

    #[test]
    fn fake_passthrough_validates_and_records() {
        let f = FakeBackend::with_inputs(&["Speakers", "CABLE Input"], &["Headset Mic", "Webcam Mic"]);
        let inputs = f.input_devices();
        assert_eq!(inputs.len(), 2);
        assert!(inputs[0].is_default && !inputs[1].is_default);
        let cable = DeviceTarget::Named("CABLE Input".into());
        let err = f.start_passthrough(&DeviceTarget::Named("Ghost Mic".into()), &cable, 1.0).unwrap_err();
        assert!(matches!(err, AudioError::InputDeviceNotFound(n) if n == "Ghost Mic"));
        let err = f.start_passthrough(&DeviceTarget::Default, &DeviceTarget::Named("Ghost".into()), 1.0).unwrap_err();
        assert!(matches!(err, AudioError::DeviceNotFound(n) if n == "Ghost"));
        assert!(!f.passthrough_active(), "failed starts leave it inactive");
        f.start_passthrough(&DeviceTarget::Named("Headset Mic".into()), &cable, 0.5).unwrap();
        assert!(f.passthrough_active());
        f.set_passthrough_gain(1.5);
        f.stop_passthrough();
        assert!(!f.passthrough_active());
        assert_eq!(
            f.calls(),
            vec![
                Call::StartPassthrough { input: DeviceTarget::Named("Headset Mic".into()), output: cable, gain: 0.5 },
                Call::SetPassthroughGain(1.5),
                Call::StopPassthrough,
            ]
        );
    }
}
