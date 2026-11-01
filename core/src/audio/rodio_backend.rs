use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use rodio::buffer::SamplesBuffer;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::{OutputStream, OutputStreamHandle, Sink};

use super::backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};

/// How long a cached stream for a named device is trusted before the device list is checked again.
const REVALIDATE_AFTER: Duration = Duration::from_secs(2);

/// Pure rate-limit decision: re-enumerate only when never validated or the last check is older than the interval.
fn needs_revalidation(last: Option<Instant>, now: Instant) -> bool {
    match last {
        None => true,
        Some(t) => now.saturating_duration_since(t) > REVALIDATE_AFTER,
    }
}

enum Cmd {
    Devices(Sender<Vec<DeviceInfo>>),
    Play { target: DeviceTarget, buffer: DecodedBuffer, gain: f32, reply: Sender<Result<SinkHandle, AudioError>> },
    Stop(SinkHandle),
    SetGain(SinkHandle, f32),
    IsFinished(SinkHandle, Sender<bool>),
    Reset,
}

/// Real backend. All cpal/rodio objects live on the `audio` thread.
pub struct RodioBackend {
    tx: Sender<Cmd>,
}

impl RodioBackend {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || audio_thread(rx))
            .expect("spawn audio thread");
        Self { tx }
    }

    fn gone() -> AudioError {
        AudioError::Stream("audio thread is gone".into())
    }
}

impl Default for RodioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBackend for RodioBackend {
    fn devices(&self) -> Vec<DeviceInfo> {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::Devices(tx)).is_err() {
            return Vec::new();
        }
        rx.recv().unwrap_or_default()
    }

    fn play(&self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Cmd::Play { target: target.clone(), buffer: buffer.clone(), gain, reply: tx })
            .map_err(|_| Self::gone())?;
        rx.recv().map_err(|_| Self::gone())?
    }

    fn stop(&self, handle: SinkHandle) {
        let _ = self.tx.send(Cmd::Stop(handle));
    }

    fn set_gain(&self, handle: SinkHandle, gain: f32) {
        let _ = self.tx.send(Cmd::SetGain(handle, gain));
    }

    fn is_finished(&self, handle: SinkHandle) -> bool {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::IsFinished(handle, tx)).is_err() {
            return true;
        }
        rx.recv().unwrap_or(true)
    }

    fn reset(&self) {
        let _ = self.tx.send(Cmd::Reset);
    }
}

struct AudioThread {
    host: rodio::cpal::Host,
    streams: HashMap<DeviceTarget, (OutputStream, OutputStreamHandle)>,
    sinks: HashMap<SinkHandle, Sink>,
    sink_targets: HashMap<SinkHandle, DeviceTarget>,
    last_validated: HashMap<DeviceTarget, Instant>,
    next: u64,
}

fn audio_thread(rx: Receiver<Cmd>) {
    let mut st = AudioThread {
        host: rodio::cpal::default_host(),
        streams: HashMap::new(),
        sinks: HashMap::new(),
        sink_targets: HashMap::new(),
        last_validated: HashMap::new(),
        next: 1,
    };
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Devices(reply) => {
                let _ = reply.send(st.devices());
            }
            Cmd::Play { target, buffer, gain, reply } => {
                let _ = reply.send(st.play(&target, &buffer, gain));
            }
            Cmd::Stop(h) => {
                if let Some(s) = st.remove_sink(h) {
                    s.stop();
                }
            }
            Cmd::SetGain(h, g) => {
                if let Some(s) = st.sinks.get(&h) {
                    s.set_volume(g);
                }
            }
            Cmd::IsFinished(h, reply) => {
                let done = st.sinks.get(&h).map(|s| s.empty()).unwrap_or(true);
                if done {
                    st.remove_sink(h);
                }
                let _ = reply.send(done);
            }
            Cmd::Reset => {
                st.sinks.clear();
                st.sink_targets.clear();
                st.streams.clear();
                st.last_validated.clear();
            }
        }
    }
}

impl AudioThread {
    fn devices(&self) -> Vec<DeviceInfo> {
        let default = self.host.default_output_device().and_then(|d| d.name().ok());
        match self.host.output_devices() {
            Ok(iter) => iter
                .filter_map(|d| d.name().ok())
                .map(|name| DeviceInfo { is_default: Some(&name) == default.as_ref(), name })
                .collect(),
            Err(e) => {
                log::warn!("enumerating output devices failed: {e}");
                Vec::new()
            }
        }
    }

    fn find_device(&self, target: &DeviceTarget) -> Result<rodio::cpal::Device, AudioError> {
        match target {
            DeviceTarget::Default => self
                .host
                .default_output_device()
                .ok_or_else(|| AudioError::Stream("no default output device".into())),
            DeviceTarget::Named(name) => self
                .host
                .output_devices()
                .map_err(|e| AudioError::Stream(e.to_string()))?
                .find(|d| d.name().map(|n| &n == name).unwrap_or(false))
                .ok_or_else(|| AudioError::DeviceNotFound(name.clone())),
        }
    }

    fn remove_sink(&mut self, h: SinkHandle) -> Option<Sink> {
        self.sink_targets.remove(&h);
        self.sinks.remove(&h)
    }

    /// Drop the stream for `target` and every sink created on it, so those voices report finished.
    fn drop_stream(&mut self, target: &DeviceTarget) {
        self.streams.remove(target);
        self.last_validated.remove(target);
        let dead: Vec<SinkHandle> =
            self.sink_targets.iter().filter(|(_, t)| *t == target).map(|(h, _)| *h).collect();
        for h in dead {
            self.remove_sink(h);
        }
    }

    fn handle_for(&mut self, target: &DeviceTarget) -> Result<OutputStreamHandle, AudioError> {
        if self.streams.contains_key(target) {
            // cpal never tells us a stream died, so named devices are re-checked now and then.
            if matches!(target, DeviceTarget::Named(_))
                && needs_revalidation(self.last_validated.get(target).copied(), Instant::now())
            {
                match self.find_device(target) {
                    Err(not_found @ AudioError::DeviceNotFound(_)) => {
                        self.drop_stream(target);
                        return Err(not_found);
                    }
                    // Enumeration trouble is not proof the device is gone; keep the stream.
                    _ => {
                        self.last_validated.insert(target.clone(), Instant::now());
                    }
                }
            }
        } else {
            let device = self.find_device(target)?;
            let (stream, handle) =
                OutputStream::try_from_device(&device).map_err(|e| AudioError::Stream(e.to_string()))?;
            self.streams.insert(target.clone(), (stream, handle));
            self.last_validated.insert(target.clone(), Instant::now());
        }
        Ok(self.streams[target].1.clone())
    }

    fn play(&mut self, target: &DeviceTarget, buffer: &DecodedBuffer, gain: f32) -> Result<SinkHandle, AudioError> {
        let sink = match self.handle_for(target).and_then(|h| Sink::try_new(&h).map_err(|e| AudioError::Stream(e.to_string()))) {
            Ok(s) => s,
            Err(e) => {
                // The stream may have died (device unplugged). Drop it so the next attempt reopens,
                // and report DeviceNotFound if the device is really gone so the engine can fall back.
                self.drop_stream(target);
                return match self.find_device(target) {
                    Err(not_found @ AudioError::DeviceNotFound(_)) => Err(not_found),
                    _ => Err(e),
                };
            }
        };
        sink.set_volume(gain);
        sink.append(SamplesBuffer::new(buffer.channels, buffer.sample_rate, (*buffer.samples).clone()));
        let h = SinkHandle(self.next);
        self.next += 1;
        self.sinks.insert(h, sink);
        self.sink_targets.insert(h, target.clone());
        Ok(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_does_not_panic_without_hardware() {
        let b = RodioBackend::new();
        let _ = b.devices();
        b.reset();
    }

    /// Needs a real output device. Run with: cargo test -p soundboard-core plays_tone -- --ignored --nocapture
    #[test]
    #[ignore]
    fn plays_tone_on_default_device() {
        let b = RodioBackend::new();
        let tone = crate::audio::engine::sine(440.0, 0.3, 44100);
        let h = b.play(&DeviceTarget::Default, &tone, 0.3).unwrap();
        assert!(!b.is_finished(h));
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(b.is_finished(h));
    }

    #[test]
    fn revalidation_is_rate_limited() {
        let now = Instant::now();
        assert!(needs_revalidation(None, now));
        assert!(!needs_revalidation(Some(now), now));
        assert!(!needs_revalidation(Some(now), now + Duration::from_millis(1500)));
        assert!(!needs_revalidation(Some(now), now + REVALIDATE_AFTER));
        assert!(needs_revalidation(Some(now), now + REVALIDATE_AFTER + Duration::from_millis(1)));
    }

    #[test]
    fn unknown_named_device_is_not_found() {
        let b = RodioBackend::new();
        let tone = crate::audio::engine::sine(440.0, 0.01, 44100);
        match b.play(&DeviceTarget::Named("definitely-not-a-device".into()), &tone, 0.1) {
            Err(AudioError::DeviceNotFound(_)) => {}
            Err(AudioError::Stream(_)) => {} // machines with no audio host at all
            other => panic!("unexpected {other:?}"),
        }
    }
}
