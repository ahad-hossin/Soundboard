use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;

use crate::config::Trim;

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

/// Interleaved 16-bit PCM, shared so the cache and active sinks never copy.
#[derive(Debug, Clone)]
pub struct DecodedBuffer {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Arc<Vec<i16>>,
}

/// 16-bit sample to f32 in [-1, 1).
pub fn to_f32(s: i16) -> f32 {
    s as f32 / 32768.0
}

impl DecodedBuffer {
    /// Converts f32 samples (clamped to [-1, 1]) to 16-bit.
    pub fn new(channels: u16, sample_rate: u32, samples: Vec<f32>) -> Self {
        let samples = samples.into_iter().map(|x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16).collect();
        Self::from_i16(channels, sample_rate, samples)
    }

    pub fn from_i16(channels: u16, sample_rate: u32, samples: Vec<i16>) -> Self {
        Self { channels, sample_rate, samples: Arc::new(samples) }
    }

    pub fn byte_len(&self) -> usize {
        self.samples.len() * std::mem::size_of::<i16>()
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
    /// Like `play`, but decodes `path` while it plays (the `trim` range, bass-boosted by `bass`)
    /// instead of holding it in memory. The handle works like one from `play`.
    fn play_stream(&self, target: &DeviceTarget, path: &Path, trim: Option<Trim>, bass: f32, gain: f32) -> Result<SinkHandle, AudioError>;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_converts_f32_to_16_bit() {
        let b = DecodedBuffer::new(1, 10, vec![0.0, 1.0, -1.0, 2.0, 0.5]);
        assert_eq!(*b.samples, vec![0, 32767, -32767, 32767, 16384]);
        assert_eq!(b.byte_len(), 10);
    }

    #[test]
    fn to_f32_maps_full_scale() {
        assert_eq!(to_f32(-32768), -1.0);
        assert_eq!(to_f32(16384), 0.5);
    }
}
