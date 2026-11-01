use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{FromSample, SampleFormat, SizedSample};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

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

/// Zero-copy rodio source over a cached decoded buffer: every sink shares the same `Arc`.
struct SharedSource {
    samples: Arc<Vec<f32>>,
    pos: usize,
    channels: u16,
    sample_rate: u32,
}

impl SharedSource {
    fn new(buffer: &DecodedBuffer) -> Self {
        Self { samples: Arc::clone(&buffer.samples), pos: 0, channels: buffer.channels, sample_rate: buffer.sample_rate }
    }
}

impl Iterator for SharedSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let s = self.samples.get(self.pos).copied()?;
        self.pos += 1;
        Some(s)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.samples.len().saturating_sub(self.pos);
        (left, Some(left))
    }
}

impl Source for SharedSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let frames = self.samples.len() as u64 / u64::from(self.channels.max(1));
        let rate = u64::from(self.sample_rate.max(1));
        Some(Duration::from_secs(frames / rate) + Duration::from_nanos((frames % rate) * 1_000_000_000 / rate))
    }
}

/// Live microphone samples for the virtual mic sink, read from the ring the input callback fills.
///
/// Never ends. Output stays silent until `prime_level` samples (20 ms) are queued, so the
/// capture and playback callbacks have slack against each other's jitter; an underrun
/// (empty at a frame boundary) goes back to silence and re-primes. When more than
/// `high_water` samples (100 ms) are queued the backlog is cut to about `high_water / 2`,
/// so latency cannot creep upward. Silence and skips happen on whole frames so interleaved
/// channels never swap.
struct MicSource {
    cons: HeapCons<f32>,
    channels: u16,
    sample_rate: u32,
    high_water: usize,
    prime_level: usize,
    /// Enough samples were queued to start reading; cleared by an underrun.
    primed: bool,
    /// Index of the next sample within the current frame.
    pos: u16,
    /// The current frame is being filled with silence.
    silent: bool,
}

impl MicSource {
    fn new(cons: HeapCons<f32>, channels: u16, sample_rate: u32) -> Self {
        let channels = channels.max(1);
        let high_water = frames_for_ms(sample_rate, 100) * usize::from(channels);
        let prime_level = frames_for_ms(sample_rate, 20) * usize::from(channels);
        Self { cons, channels, sample_rate, high_water, prime_level, primed: false, pos: 0, silent: true }
    }
}

/// Whole frames in `ms` milliseconds at `sample_rate`, at least one.
fn frames_for_ms(sample_rate: u32, ms: u32) -> usize {
    (sample_rate as usize * ms as usize / 1000).max(1)
}

impl Iterator for MicSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let channels = usize::from(self.channels);
        if self.pos == 0 {
            let occupied = self.cons.occupied_len();
            if occupied > self.high_water {
                let excess = occupied - self.high_water / 2;
                self.cons.skip(excess - excess % channels);
            }
            let occupied = self.cons.occupied_len();
            if self.primed && occupied < channels {
                self.primed = false; // underrun: wait for a fresh cushion
            }
            if !self.primed && occupied >= self.prime_level {
                self.primed = true;
            }
            self.silent = !self.primed;
        }
        let sample = if self.silent { 0.0 } else { self.cons.try_pop().unwrap_or(0.0) };
        self.pos = (self.pos + 1) % self.channels;
        Some(sample)
    }
}

impl Source for MicSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Pushes whole frames of `data` into the ring as f32; frames that do not fit are dropped.
fn push_frames<T>(prod: &mut HeapProd<f32>, data: &[T], channels: usize)
where
    T: Copy,
    f32: FromSample<T>,
{
    for frame in data.chunks_exact(channels) {
        if prod.vacant_len() < channels {
            break;
        }
        for &s in frame {
            let _ = prod.try_push(f32::from_sample_(s));
        }
    }
}

fn build_mic_stream<T>(
    device: &rodio::cpal::Device,
    config: &rodio::cpal::StreamConfig,
    mut prod: HeapProd<f32>,
) -> Result<rodio::cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| push_frames(&mut prod, data, channels),
            |e| log::warn!("microphone stream error: {e}"),
            None,
        )
        .map_err(|e| AudioError::Stream(e.to_string()))
}

/// A running mic passthrough. Field order matters: the sink stops before the capture stream drops.
struct Passthrough {
    sink: Sink,
    _input: rodio::cpal::Stream,
    output: DeviceTarget,
}

impl Drop for Passthrough {
    fn drop(&mut self) {
        self.sink.stop();
    }
}

enum Cmd {
    Devices(Sender<Vec<DeviceInfo>>),
    InputDevices(Sender<Vec<DeviceInfo>>),
    StartPassthrough { input: DeviceTarget, output: DeviceTarget, gain: f32, reply: Sender<Result<(), AudioError>> },
    StopPassthrough,
    SetPassthroughGain(f32),
    PassthroughActive(Sender<bool>),
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

    fn input_devices(&self) -> Vec<DeviceInfo> {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::InputDevices(tx)).is_err() {
            return Vec::new();
        }
        rx.recv().unwrap_or_default()
    }

    fn start_passthrough(&self, input: &DeviceTarget, output: &DeviceTarget, gain: f32) -> Result<(), AudioError> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Cmd::StartPassthrough { input: input.clone(), output: output.clone(), gain, reply: tx })
            .map_err(|_| Self::gone())?;
        rx.recv().map_err(|_| Self::gone())?
    }

    fn stop_passthrough(&self) {
        let _ = self.tx.send(Cmd::StopPassthrough);
    }

    fn set_passthrough_gain(&self, gain: f32) {
        let _ = self.tx.send(Cmd::SetPassthroughGain(gain));
    }

    fn passthrough_active(&self) -> bool {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(Cmd::PassthroughActive(tx)).is_err() {
            return false;
        }
        rx.recv().unwrap_or(false)
    }
}

struct AudioThread {
    host: rodio::cpal::Host,
    streams: HashMap<DeviceTarget, (OutputStream, OutputStreamHandle)>,
    sinks: HashMap<SinkHandle, Sink>,
    sink_targets: HashMap<SinkHandle, DeviceTarget>,
    last_validated: HashMap<DeviceTarget, Instant>,
    passthrough: Option<Passthrough>,
    next: u64,
}

fn audio_thread(rx: Receiver<Cmd>) {
    let mut st = AudioThread {
        host: rodio::cpal::default_host(),
        streams: HashMap::new(),
        sinks: HashMap::new(),
        sink_targets: HashMap::new(),
        last_validated: HashMap::new(),
        passthrough: None,
        next: 1,
    };
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Devices(reply) => {
                let _ = reply.send(st.devices());
            }
            Cmd::InputDevices(reply) => {
                let _ = reply.send(st.input_devices());
            }
            Cmd::StartPassthrough { input, output, gain, reply } => {
                let _ = reply.send(st.start_passthrough(&input, &output, gain));
            }
            Cmd::StopPassthrough => {
                st.passthrough = None;
            }
            Cmd::SetPassthroughGain(g) => {
                if let Some(p) = &st.passthrough {
                    p.sink.set_volume(g);
                }
            }
            Cmd::PassthroughActive(reply) => {
                let _ = reply.send(st.passthrough.is_some());
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
                st.passthrough = None;
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

    fn input_devices(&self) -> Vec<DeviceInfo> {
        let default = self.host.default_input_device().and_then(|d| d.name().ok());
        match self.host.input_devices() {
            Ok(iter) => iter
                .filter_map(|d| d.name().ok())
                .map(|name| DeviceInfo { is_default: Some(&name) == default.as_ref(), name })
                .collect(),
            Err(e) => {
                log::warn!("enumerating input devices failed: {e}");
                Vec::new()
            }
        }
    }

    fn find_input_device(&self, target: &DeviceTarget) -> Result<rodio::cpal::Device, AudioError> {
        match target {
            DeviceTarget::Default => self
                .host
                .default_input_device()
                .ok_or_else(|| AudioError::InputDeviceNotFound("default microphone".into())),
            DeviceTarget::Named(name) => self
                .host
                .input_devices()
                .map_err(|e| AudioError::Stream(e.to_string()))?
                .find(|d| d.name().map(|n| &n == name).unwrap_or(false))
                .ok_or_else(|| AudioError::InputDeviceNotFound(name.clone())),
        }
    }

    /// Captures `input` into a 200 ms ring and plays it through a sink on the shared `output` stream.
    fn start_passthrough(&mut self, input: &DeviceTarget, output: &DeviceTarget, gain: f32) -> Result<(), AudioError> {
        self.passthrough = None;
        let device = self.find_input_device(input)?;
        let supported = device.default_input_config().map_err(|e| AudioError::Stream(e.to_string()))?;
        let config = supported.config();
        let channels = config.channels.max(1);
        let sample_rate = config.sample_rate.0;
        let capacity = frames_for_ms(sample_rate, 200) * usize::from(channels);
        let (prod, cons) = HeapRb::<f32>::new(capacity).split();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_mic_stream::<f32>(&device, &config, prod)?,
            SampleFormat::I16 => build_mic_stream::<i16>(&device, &config, prod)?,
            SampleFormat::U16 => build_mic_stream::<u16>(&device, &config, prod)?,
            _ => return Err(AudioError::Stream("unsupported mic sample format".into())),
        };
        stream.play().map_err(|e| AudioError::Stream(e.to_string()))?;
        let sink = match self.handle_for(output).and_then(|h| Sink::try_new(&h).map_err(|e| AudioError::Stream(e.to_string()))) {
            Ok(s) => s,
            Err(e) => {
                // Same recovery as `play`: reopen next time, and say so plainly if the device is gone.
                self.drop_stream(output);
                return match self.find_device(output) {
                    Err(not_found @ AudioError::DeviceNotFound(_)) => Err(not_found),
                    _ => Err(e),
                };
            }
        };
        sink.set_volume(gain);
        sink.append(MicSource::new(cons, channels, sample_rate));
        self.passthrough = Some(Passthrough { sink, _input: stream, output: output.clone() });
        Ok(())
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
    /// A passthrough playing on it is dropped too; the engine restarts it on the next device change.
    fn drop_stream(&mut self, target: &DeviceTarget) {
        if self.passthrough.as_ref().is_some_and(|p| &p.output == target) {
            self.passthrough = None;
        }
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
        sink.append(SharedSource::new(buffer));
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
    fn shared_source_yields_the_buffer_without_copying() {
        let buffer = DecodedBuffer::new(2, 4, vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6, 0.7, -0.8, 0.9, -1.0]);
        let src = SharedSource::new(&buffer);
        assert!(Arc::ptr_eq(&src.samples, &buffer.samples), "shares the cached Arc");
        assert_eq!(src.channels(), 2);
        assert_eq!(src.sample_rate(), 4);
        assert_eq!(src.current_frame_len(), None);
        // 10 samples / 2 channels = 5 frames at 4 Hz = 1.25 s
        assert_eq!(src.total_duration(), Some(Duration::from_millis(1250)));
        assert_eq!(src.size_hint(), (10, Some(10)));
        let out: Vec<f32> = src.collect();
        assert_eq!(out, *buffer.samples);
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

    fn ring(capacity: usize) -> (HeapProd<f32>, HeapCons<f32>) {
        HeapRb::<f32>::new(capacity).split()
    }

    fn push_n(prod: &mut HeapProd<f32>, from: usize, n: usize) {
        for i in from..from + n {
            prod.try_push(i as f32).unwrap();
        }
    }

    #[test]
    fn mic_source_yields_pushed_samples_then_silence() {
        // 1000 Hz mono: prime level 20 samples.
        let (mut prod, cons) = ring(1000);
        push_n(&mut prod, 0, 25);
        let mut src = MicSource::new(cons, 1, 1000);
        assert_eq!(src.channels(), 1);
        assert_eq!(src.sample_rate(), 1000);
        assert_eq!(src.current_frame_len(), None);
        assert_eq!(src.total_duration(), None);
        let out: Vec<f32> = src.by_ref().take(25).collect();
        assert_eq!(out, (0..25).map(|i| i as f32).collect::<Vec<_>>());
        assert_eq!(src.next(), Some(0.0), "underrun is silence, not the end");
        assert_eq!(src.next(), Some(0.0));
    }

    #[test]
    fn mic_source_primes_before_reading_and_re_primes_after_underrun() {
        let (mut prod, cons) = ring(1000);
        let mut src = MicSource::new(cons, 1, 1000);
        assert_eq!(src.prime_level, 20);
        push_n(&mut prod, 1, 19);
        for _ in 0..5 {
            assert_eq!(src.next(), Some(0.0), "below prime level: silence");
        }
        assert_eq!(src.cons.occupied_len(), 19, "nothing consumed while priming");
        push_n(&mut prod, 20, 1);
        let out: Vec<f32> = src.by_ref().take(20).collect();
        assert_eq!(out, (1..=20).map(|i| i as f32).collect::<Vec<_>>(), "primed: reads everything");
        // Drained to empty: the next frame is silence and a single new sample does not restart it.
        assert_eq!(src.next(), Some(0.0));
        push_n(&mut prod, 100, 1);
        assert_eq!(src.next(), Some(0.0), "re-priming");
        push_n(&mut prod, 101, 19);
        assert_eq!(src.next(), Some(100.0), "primed again");
    }

    #[test]
    fn mic_source_drops_backlog_past_high_water() {
        // 1000 Hz mono: high water is 100 samples.
        let (mut prod, cons) = ring(1000);
        let mut src = MicSource::new(cons, 1, 1000);
        assert_eq!(src.high_water, 100);
        push_n(&mut prod, 0, 300);
        let first = src.next().unwrap();
        // 300 queued, cut to 50: the next sample comes from the tail.
        assert_eq!(first, 250.0);
        assert_eq!(src.cons.occupied_len(), 49);
        assert_eq!(src.next(), Some(251.0));
    }

    #[test]
    fn mic_source_keeps_stereo_frames_aligned() {
        // 1000 Hz stereo: prime level 40 samples, high water 200.
        let (mut prod, cons) = ring(1000);
        let mut src = MicSource::new(cons, 2, 1000);
        assert_eq!((src.prime_level, src.high_water), (40, 200));
        let stereo = |prod: &mut HeapProd<f32>, from: usize, n: usize| {
            for i in from..from + n {
                prod.try_push(i as f32 + 1.0).unwrap();
                prod.try_push(-(i as f32) - 1.0).unwrap();
            }
        };
        // 20 frames plus a lone left sample: 20 frames play, then the half frame waits.
        stereo(&mut prod, 0, 20);
        prod.try_push(1000.0).unwrap();
        let out: Vec<f32> = src.by_ref().take(40).collect();
        assert!(out.chunks(2).all(|f| f[0] > 0.0 && f[1] == -f[0]), "{out:?}");
        assert_eq!(src.next(), Some(0.0));
        assert_eq!(src.next(), Some(0.0));
        assert_eq!(src.cons.occupied_len(), 1, "the half frame is not played alone");
        // Backlog skips cut whole frames: left samples stay positive, right negative.
        let (mut prod, cons) = ring(1000);
        let mut src = MicSource::new(cons, 2, 1000);
        stereo(&mut prod, 0, 301);
        let l = src.next().unwrap();
        let r = src.next().unwrap();
        assert!(l > 0.0 && r < 0.0 && l == -r, "l={l} r={r}");
        assert!(src.cons.occupied_len() <= 100);
    }

    #[test]
    fn push_frames_drops_frames_that_do_not_fit() {
        let (mut prod, mut cons) = ring(5);
        push_frames(&mut prod, &[1i16, 2, 3, 4, 5, 6], 2);
        assert_eq!(cons.occupied_len(), 4, "the third frame does not fit whole");
        let first = cons.try_pop().unwrap();
        assert!((first - 1.0 / 32768.0).abs() < 1e-6, "i16 converted to f32: {first}");
    }

    #[test]
    fn input_devices_does_not_panic() {
        let b = RodioBackend::new();
        let _ = b.input_devices();
        b.stop_passthrough();
        b.set_passthrough_gain(0.5);
        assert!(!b.passthrough_active());
    }

    #[test]
    fn unknown_named_microphone_is_not_found() {
        let b = RodioBackend::new();
        match b.start_passthrough(&DeviceTarget::Named("definitely-not-a-mic".into()), &DeviceTarget::Default, 1.0) {
            Err(AudioError::InputDeviceNotFound(_)) => {}
            Err(AudioError::Stream(_)) => {} // machines with no audio host at all
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Needs a real microphone and output device; you may hear yourself for 300 ms.
    /// Run with: cargo test -p soundboard-core passthrough_default -- --ignored --nocapture
    #[test]
    #[ignore]
    fn passthrough_default_mic_to_default_output() {
        let b = RodioBackend::new();
        b.start_passthrough(&DeviceTarget::Default, &DeviceTarget::Default, 0.5).unwrap();
        assert!(b.passthrough_active());
        std::thread::sleep(std::time::Duration::from_millis(300));
        b.set_passthrough_gain(0.25);
        b.stop_passthrough();
        assert!(!b.passthrough_active());
        // The audio thread is still responsive after stopping.
        let _ = b.devices();
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
