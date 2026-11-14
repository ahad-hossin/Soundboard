use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;

/// Opaque id for one playing sink on the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SinkHandle(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceTarget {
    Default,
    Named(String),
}

impl DeviceTarget {
    pub fn from_name(name: Option<&str>) -> Self {
        match name {
            Some(n) if !n.is_empty() => DeviceTarget::Named(n.to_string()),
            _ => DeviceTarget::Default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// Interleaved f32 PCM, shared so the cache and active sinks never copy.
#[derive(Debug, Clone)]
pub struct DecodedBuffer {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Arc<Vec<f32>>,
}

impl DecodedBuffer {
    pub fn new(channels: u16, sample_rate: u32, samples: Vec<f32>) -> Self {
        Self { channels, sample_rate, samples: Arc::new(samples) }
    }

    pub fn byte_len(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / (self.channels.max(1) as f32 * self.sample_rate.max(1) as f32)
    }
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("output device not found: {0}")]
    DeviceNotFound(String),
    #[error("microphone not found: {0}")]
    InputDeviceNotFound(String),
    #[error("audio stream error: {0}")]
    Stream(String),
    #[error("could not decode {0}")]
    Decode(String),
    #[error("no output device selected")]
    NoOutputs,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub trait AudioBackend: Send + Sync {
    fn devices(&self) -> Vec<DeviceInfo>;
    /// Start playing `buffer` on `target` at linear `gain`. Returns a handle for later control.
    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError>;
    fn stop(&self, handle: SinkHandle);
    fn set_gain(&self, handle: SinkHandle, gain: f32);
    /// True once the sink has drained (or never existed).
    fn is_finished(&self, handle: SinkHandle) -> bool;
    /// Drop every open device stream; next play reopens lazily.
    fn reset(&self);
    fn input_devices(&self) -> Vec<DeviceInfo>;
    /// Start (or restart) mic passthrough: capture `input`, play it on `output` at `gain`. Replaces any running passthrough.
    fn start_passthrough(&self, input: &DeviceTarget, output: &DeviceTarget, gain: f32) -> Result<(), AudioError>;
    fn stop_passthrough(&self);
    fn set_passthrough_gain(&self, gain: f32);
    /// True while a started passthrough is still running (not stopped, reset or dropped with its output stream).
    fn passthrough_active(&self) -> bool;
    /// Open `target` in exclusive mode from now on (None: no such output). Recorders and stream
    /// tools capture every shared-mode stream a process plays, on every device, so the virtual
    /// mic (which carries the user's voice) must stay out of shared mode or the voice shows up
    /// twice in Discord screen shares and in per-app clip recordings.
    fn set_exclusive_output(&self, target: Option<DeviceTarget>);
    /// Why the exclusive output had to open in shared mode instead, or None while it is
    /// exclusive or not open. Cheap: no round trip to the audio thread.
    fn exclusive_fallback(&self) -> Option<String>;
}
