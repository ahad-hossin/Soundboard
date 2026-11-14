//! The virtual mic output in WASAPI exclusive mode.
//!
//! Medal's per-app audio tracks, Discord screen share and OBS application capture all record
//! through Windows process loopback, which picks up every shared-mode stream a process plays,
//! on every device. The virtual mic carries the user's own voice (mic passthrough), so a shared
//! stream to it puts that voice into the soundboard's captured audio, and listeners hear it
//! twice: once from the call or mic track, once from the soundboard. An exclusive-mode stream
//! bypasses the Windows mixer, so process loopback never sees it, while the cable still hands
//! it to Discord. Only the virtual mic is opened this way; the speakers stay shared.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};

use rodio::dynamic_mixer::{self, DynamicMixer, DynamicMixerController};
use rodio::Sink;
use wasapi::{
    calculate_period_100ns, deinitialize, initialize_mta, AudioClient, AudioRenderClient, Device, DeviceEnumerator, Direction,
    Handle, SampleType, StreamMode, WasapiError, WaveFormat,
};
use windows::Win32::Media::Audio::{
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED, AUDCLNT_E_UNSUPPORTED_FORMAT,
};

use super::backend::AudioError;

/// How long the render thread waits for the device to ask for more audio before it decides
/// the device is gone. The device asks every period (about 10 ms).
const DEVICE_TIMEOUT_MS: u32 = 500;

/// How one sample is laid out in the device buffer (little endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleKind {
    F32,
    I16,
    /// Packed 3 bytes.
    I24,
    /// 4 bytes; also used for 24 valid bits in a 32-bit container, which reads the top bits.
    I32,
}

impl SampleKind {
    fn of(format: &WaveFormat) -> Option<Self> {
        match (format.get_subformat().ok()?, format.get_bitspersample()) {
            (SampleType::Float, 32) => Some(Self::F32),
            (SampleType::Int, 16) => Some(Self::I16),
            (SampleType::Int, 24) => Some(Self::I24),
            (SampleType::Int, 32) => Some(Self::I32),
            _ => None,
        }
    }

    /// Appends `v`, clamped to -1..=1 (NaN as silence), in this layout.
    fn encode(self, v: f32, out: &mut Vec<u8>) {
        let v = if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) };
        match self {
            Self::F32 => out.extend_from_slice(&v.to_le_bytes()),
            Self::I16 => out.extend_from_slice(&((v * i16::MAX as f32).round() as i16).to_le_bytes()),
            Self::I24 => out.extend_from_slice(&((v * 8_388_607.0).round() as i32).to_le_bytes()[..3]),
            Self::I32 => out.extend_from_slice(&((f64::from(v) * f64::from(i32::MAX)).round() as i32).to_le_bytes()),
        }
    }
}

/// Stereo formats to offer the device, best first: its own rate, then 48 kHz and 44.1 kHz, each
/// as float and then integer from widest. As (rate, store bits, valid bits, float).
fn candidates(device_rate: u32) -> Vec<(u32, usize, usize, bool)> {
    let mut rates = Vec::new();
    for r in [device_rate, 48_000, 44_100] {
        if r > 0 && !rates.contains(&r) {
            rates.push(r);
        }
    }
    let layouts = [(32, 32, true), (32, 32, false), (24, 24, false), (32, 24, false), (16, 16, false)];
    rates.into_iter().flat_map(|r| layouts.into_iter().map(move |(s, v, f)| (r, s, v, f))).collect()
}

/// Pulls `frames` whole frames from the mixer into `out`; silence where the mixer has nothing.
fn fill(mixer: &mut impl Iterator<Item = f32>, frames: usize, channels: usize, kind: SampleKind, out: &mut Vec<u8>) {
    for _ in 0..frames * channels {
        kind.encode(mixer.next().unwrap_or(0.0), out);
    }
}

fn stream_error(e: WasapiError) -> AudioError {
    AudioError::Stream(e.to_string())
}

/// Plain-language reason for a refused exclusive open.
fn init_error(e: WasapiError) -> AudioError {
    let reason = match &e {
        WasapiError::Windows(w) if w.code() == AUDCLNT_E_DEVICE_IN_USE => "another app is using it".to_string(),
        WasapiError::Windows(w) if w.code() == AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED => {
            "exclusive mode is turned off for it (Windows Sound settings, device Properties, Advanced)".to_string()
        }
        WasapiError::Windows(w) if w.code() == AUDCLNT_E_UNSUPPORTED_FORMAT => "it refused the audio format".to_string(),
        _ => e.to_string(),
    };
    AudioError::Stream(reason)
}

/// A device opened exclusively and fed from a mixer on its own thread. Sinks made by `new_sink`
/// mix into it the same way they would on a shared rodio stream.
pub(crate) struct ExclusiveOutput {
    mixer: Arc<DynamicMixerController<f32>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ExclusiveOutput {
    /// Opens the render device named `device_name` exclusively. Blocks until it is playing or
    /// refused: `DeviceNotFound` when no such device exists, `Stream` with a plain reason otherwise.
    pub(crate) fn open(device_name: &str) -> Result<Self, AudioError> {
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::channel();
        let (name, thread_stop, thread_alive) = (device_name.to_string(), Arc::clone(&stop), Arc::clone(&alive));
        let thread = thread::Builder::new()
            .name("virtual-mic".into())
            .spawn(move || {
                let com = initialize_mta().is_ok();
                match Render::open(&name) {
                    Ok(render) => {
                        let _ = tx.send(Ok(Arc::clone(&render.controller)));
                        render.run(&thread_stop);
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
                thread_alive.store(false, Ordering::SeqCst);
                if com {
                    deinitialize();
                }
            })
            .map_err(|e| AudioError::Stream(e.to_string()))?;
        match rx.recv() {
            Ok(Ok(mixer)) => Ok(Self { mixer, stop, alive, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err(AudioError::Stream("the virtual mic thread ended while opening".into()))
            }
        }
    }

    pub(crate) fn new_sink(&self) -> Sink {
        let (sink, queue) = Sink::new_idle();
        self.mixer.add(queue);
        sink
    }

    /// False once the device stopped asking for audio (unplugged, driver restarted).
    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

impl Drop for ExclusiveOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The COM side of an exclusive output; lives and dies on the render thread.
struct Render {
    client: AudioClient,
    render: AudioRenderClient,
    event: Handle,
    kind: SampleKind,
    channels: usize,
    mixer: DynamicMixer<f32>,
    controller: Arc<DynamicMixerController<f32>>,
}

impl Render {
    fn open(name: &str) -> Result<Self, AudioError> {
        let device = DeviceEnumerator::new()
            .and_then(|e| e.get_device_collection(Direction::Render))
            .and_then(|c| c.get_device_with_name(name))
            .map_err(|e| match e {
                WasapiError::DeviceNotFound(n) => AudioError::DeviceNotFound(n),
                e => stream_error(e),
            })?;
        let mut client = device.get_iaudioclient().map_err(stream_error)?;
        let device_rate = client.get_mixformat().map(|f| f.get_samplespersec()).unwrap_or(48_000);
        let format = candidates(device_rate)
            .into_iter()
            .map(|(rate, store, valid, float)| {
                let sample_type = if float { SampleType::Float } else { SampleType::Int };
                WaveFormat::new(store, valid, sample_type, rate as usize, 2, None)
            })
            .find_map(|f| client.is_supported_exclusive_with_quirks(&f).ok())
            .ok_or_else(|| AudioError::Stream("it accepts none of the usual formats in exclusive mode".into()))?;
        let kind = SampleKind::of(&format).ok_or_else(|| AudioError::Stream("unexpected exclusive sample format".into()))?;
        let (period, _) = client.get_device_period().map_err(stream_error)?;
        Self::initialize(&device, &mut client, &format, period)?;
        let event = client.set_get_eventhandle().map_err(stream_error)?;
        let render = client.get_audiorenderclient().map_err(stream_error)?;
        let channels = format.get_nchannels();
        let (controller, mixer) = dynamic_mixer::mixer::<f32>(channels, format.get_samplespersec());
        let mut r = Self { client, render, event, kind, channels: usize::from(channels), mixer, controller };
        // Start from a full buffer so the first period is not garbage.
        r.write(&mut Vec::new()).map_err(stream_error)?;
        r.client.start_stream().map_err(stream_error)?;
        Ok(r)
    }

    /// Event-driven exclusive init, retrying once with the aligned period some devices ask for.
    fn initialize(device: &Device, client: &mut AudioClient, format: &WaveFormat, period: i64) -> Result<(), AudioError> {
        let mode = |period_hns| StreamMode::EventsExclusive { period_hns };
        match client.initialize_client(format, Direction::Render, mode(period), None) {
            Ok(()) => Ok(()),
            Err(WasapiError::Windows(w)) if w.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                let frames = client.get_buffer_size().map_err(stream_error)?;
                let aligned = calculate_period_100ns(i64::from(frames), i64::from(format.get_samplespersec()));
                *client = device.get_iaudioclient().map_err(stream_error)?;
                client.initialize_client(format, Direction::Render, mode(aligned), None).map_err(init_error)
            }
            Err(e) => Err(init_error(e)),
        }
    }

    fn run(mut self, stop: &AtomicBool) {
        let _priority = ProAudioPriority::enter();
        let mut buf = Vec::new();
        while !stop.load(Ordering::SeqCst) {
            if self.event.wait_for_event(DEVICE_TIMEOUT_MS).is_err() {
                log::warn!("virtual mic stopped asking for audio; closing it");
                break;
            }
            if let Err(e) = self.write(&mut buf) {
                log::warn!("virtual mic write failed: {e}");
                break;
            }
        }
        let _ = self.client.stop_stream();
    }

    fn write(&mut self, buf: &mut Vec<u8>) -> Result<(), WasapiError> {
        let frames = self.client.get_available_space_in_frames()? as usize;
        buf.clear();
        fill(&mut self.mixer, frames, self.channels, self.kind, buf);
        self.render.write_to_device(buf, None)
    }
}

/// Registers the render thread with the Windows multimedia scheduler, so a busy game does not
/// starve it (exclusive mode has no Windows mixer to cover for a late buffer).
struct ProAudioPriority(windows::Win32::Foundation::HANDLE);

impl ProAudioPriority {
    fn enter() -> Option<Self> {
        use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
        let mut task_index = 0u32;
        unsafe { AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut task_index) }.ok().map(Self)
    }
}

impl Drop for ProAudioPriority {
    fn drop(&mut self) {
        use windows::Win32::System::Threading::AvRevertMmThreadCharacteristics;
        let _ = unsafe { AvRevertMmThreadCharacteristics(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(kind: SampleKind, v: f32) -> Vec<u8> {
        let mut out = Vec::new();
        kind.encode(v, &mut out);
        out
    }

    #[test]
    fn encodes_each_layout_little_endian_and_clamped() {
        assert_eq!(bytes(SampleKind::F32, 0.5), 0.5f32.to_le_bytes());
        assert_eq!(bytes(SampleKind::I16, 1.0), 32767i16.to_le_bytes());
        assert_eq!(bytes(SampleKind::I16, -2.0), (-32767i16).to_le_bytes(), "clamped");
        assert_eq!(bytes(SampleKind::I24, 0.5), [0x00, 0x00, 0x40], "4194304 packed in 3 bytes");
        assert_eq!(bytes(SampleKind::I24, -1.0), [0x01, 0x00, 0x80]);
        assert_eq!(bytes(SampleKind::I32, -1.0), (-i32::MAX).to_le_bytes());
        assert_eq!(bytes(SampleKind::I32, f32::NAN), 0i32.to_le_bytes(), "NaN is silence");
    }

    #[test]
    fn candidates_start_at_the_device_rate_without_repeats() {
        let c = candidates(44_100);
        assert_eq!(c[0], (44_100, 32, 32, true), "device rate, float first");
        let rates: Vec<u32> = c.iter().map(|x| x.0).collect();
        assert_eq!(rates.iter().filter(|&&r| r == 44_100).count(), 5, "44.1 kHz offered once per layout");
        assert_eq!(rates.last(), Some(&48_000));
        let c = candidates(0);
        assert_eq!(c[0].0, 48_000, "an unknown device rate starts at 48 kHz");
        assert_eq!(c.len(), 10);
    }

    #[test]
    fn fill_pulls_whole_frames_and_pads_with_silence() {
        let mut mixer = vec![0.5f32, -0.5, 0.25].into_iter();
        let mut out = Vec::new();
        fill(&mut mixer, 3, 2, SampleKind::I16, &mut out);
        assert_eq!(out.len(), 3 * 2 * 2);
        let samples: Vec<i16> = out.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(samples, vec![16384, -16384, 8192, 0, 0, 0]);
    }

    /// Process loopback capture of this test process, plus a plain capture of CABLE Output;
    /// returns the peak level each one heard over `secs`.
    fn listen(secs: f32) -> (f32, f32) {
        fn peak(mut client: AudioClient, secs: f32) -> f32 {
            let format = WaveFormat::new(32, 32, SampleType::Float, 48_000, 2, None);
            let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 };
            client.initialize_client(&format, Direction::Capture, mode, None).unwrap();
            let event = client.set_get_eventhandle().unwrap();
            let capture = client.get_audiocaptureclient().unwrap();
            let mut data = std::collections::VecDeque::new();
            client.start_stream().unwrap();
            let end = std::time::Instant::now() + std::time::Duration::from_secs_f32(secs);
            while std::time::Instant::now() < end {
                let _ = event.wait_for_event(200);
                while capture.get_next_packet_size().unwrap().unwrap_or(0) > 0 {
                    capture.read_from_device_to_deque(&mut data).unwrap();
                }
            }
            let bytes: Vec<u8> = data.into_iter().collect();
            bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs()).fold(0.0, f32::max)
        }
        let pid = std::process::id();
        let app = thread::spawn(move || {
            let _ = initialize_mta();
            peak(AudioClient::new_application_loopback_client(pid, true).unwrap(), secs)
        });
        let cable = thread::spawn(move || {
            let _ = initialize_mta();
            let device = DeviceEnumerator::new()
                .unwrap()
                .get_device_collection(Direction::Capture)
                .unwrap()
                .get_device_with_name("CABLE Output (VB-Audio Virtual Cable)")
                .unwrap();
            peak(device.get_iaudioclient().unwrap(), secs)
        });
        (app.join().unwrap(), cable.join().unwrap())
    }

    /// Needs VB-Cable. Plays a quiet tone (-50 dBFS) into CABLE Input, first shared through
    /// rodio as before, then exclusively. Per-app capture must hear the shared one only; the
    /// cable must carry both. Run with:
    /// cargo test -p soundboard-core exclusive_virtual_mic -- --ignored --nocapture
    #[test]
    #[ignore]
    fn exclusive_virtual_mic_is_invisible_to_per_app_capture() {
        use rodio::cpal::traits::{DeviceTrait, HostTrait};
        use rodio::source::{SineWave, Source};
        const CABLE: &str = "CABLE Input (VB-Audio Virtual Cable)";
        let tone = || SineWave::new(440.0).amplify(0.003).take_duration(std::time::Duration::from_secs(3));

        let host = rodio::cpal::default_host();
        let device = host.output_devices().unwrap().find(|d| d.name().map(|n| n == CABLE).unwrap_or(false)).expect("VB-Cable");
        let (_stream, handle) = rodio::OutputStream::try_from_device(&device).unwrap();
        let shared = Sink::try_new(&handle).unwrap();
        shared.append(tone());
        std::thread::sleep(std::time::Duration::from_millis(300));
        let (app, cable) = listen(2.0);
        println!("shared:    per-app capture peak {app:.4}, CABLE Output peak {cable:.4}");
        assert!(cable > 0.002, "the cable carries the shared stream");
        assert!(app > 0.002, "control: per-app capture hears a shared stream (the bug)");
        drop((shared, _stream));

        let out = ExclusiveOutput::open(CABLE).expect("exclusive open");
        let sink = out.new_sink();
        sink.append(tone());
        std::thread::sleep(std::time::Duration::from_millis(300));
        let (app, cable) = listen(2.0);
        println!("exclusive: per-app capture peak {app:.4}, CABLE Output peak {cable:.4}");
        assert!(cable > 0.002, "the cable carries the exclusive stream");
        assert!(app < 0.0005, "per-app capture must not hear the exclusive stream");
        assert!(out.is_alive());
    }
}
