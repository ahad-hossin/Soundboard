//! Helpers shared by unit tests. Compiled only under `cfg(test)`.

use std::path::Path;

/// Writes a 16-bit stereo 44.1 kHz WAV with `frames` frames of a quiet sawtooth.
pub fn write_test_wav(path: &Path, frames: u32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..frames {
        let v = ((i % 100) as i16 - 50) * 100;
        w.write_sample(v).unwrap();
        w.write_sample(v).unwrap();
    }
    w.finalize().unwrap();
}
