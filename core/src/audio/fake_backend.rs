use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Play { handle: SinkHandle, target: DeviceTarget, gain: f32, samples: usize },
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
    inputs: Vec<DeviceInfo>,
    next: AtomicU64,
    calls: Mutex<Vec<Call>>,
    finished: Mutex<HashSet<SinkHandle>>,
    gains: Mutex<HashMap<SinkHandle, f32>>,
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
            inputs: infos(inputs),
            next: AtomicU64::new(1),
            calls: Mutex::new(Vec::new()),
            finished: Mutex::new(HashSet::new()),
            gains: Mutex::new(HashMap::new()),
        }
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// Handles in play order.
    pub fn handles(&self) -> Vec<SinkHandle> {
        self.calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Play { handle, .. } => Some(handle),
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
}

impl AudioBackend for FakeBackend {
    fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.clone()
    }

    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        if let DeviceTarget::Named(n) = target {
            if !self.devices.iter().any(|d| &d.name == n) {
                return Err(AudioError::DeviceNotFound(n.clone()));
            }
        }
        let handle = SinkHandle(self.next.fetch_add(1, Ordering::SeqCst));
        self.calls.lock().unwrap().push(Call::Play {
            handle,
            target: target.clone(),
            gain,
            samples: buffer.samples.len(),
        });
        self.gains.lock().unwrap().insert(handle, gain);
        Ok(handle)
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
    }

    fn input_devices(&self) -> Vec<DeviceInfo> {
        self.inputs.clone()
    }

    fn start_passthrough(&self, input: &DeviceTarget, output: &DeviceTarget, gain: f32) -> Result<(), AudioError> {
        if let DeviceTarget::Named(n) = input {
            if !self.inputs.iter().any(|d| &d.name == n) {
                return Err(AudioError::InputDeviceNotFound(n.clone()));
            }
        }
        if let DeviceTarget::Named(n) = output {
            if !self.devices.iter().any(|d| &d.name == n) {
                return Err(AudioError::DeviceNotFound(n.clone()));
            }
        }
        self.calls.lock().unwrap().push(Call::StartPassthrough { input: input.clone(), output: output.clone(), gain });
        Ok(())
    }

    fn stop_passthrough(&self) {
        self.calls.lock().unwrap().push(Call::StopPassthrough);
    }

    fn set_passthrough_gain(&self, gain: f32) {
        self.calls.lock().unwrap().push(Call::SetPassthroughGain(gain));
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
        f.start_passthrough(&DeviceTarget::Named("Headset Mic".into()), &cable, 0.5).unwrap();
        f.set_passthrough_gain(1.5);
        f.stop_passthrough();
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
