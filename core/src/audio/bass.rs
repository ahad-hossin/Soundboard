//! Non-destructive bass boost: a low-shelf filter plus soft saturation, applied to a decoded buffer.

use super::backend::{to_f32, DecodedBuffer};

const SHELF_HZ: f64 = 120.0;
const MAX_BOOST_DB: f64 = 18.0;

/// Bass-boosted copy of `buffer`. `strength` 0..=1 scales the shelf gain (up to +18 dB) and the
/// saturation drive; the result is scaled down if needed so its peak is at most 1.0.
/// Strength 0 (or non-finite) returns `buffer` itself, sharing its samples.
pub fn bass_boost(buffer: &DecodedBuffer, strength: f32) -> DecodedBuffer {
    let Some(mut filter) = BassFilter::new(buffer.channels, buffer.sample_rate, strength) else {
        return buffer.clone();
    };
    let mut out: Vec<f32> = buffer.samples.iter().map(|&x| filter.process(to_f32(x))).collect();
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 1.0 {
        out.iter_mut().for_each(|s| *s /= peak);
    }
    DecodedBuffer::new(buffer.channels, buffer.sample_rate, out)
}

/// The boost as a stream over interleaved samples: shelf biquad plus `tanh` saturation, one
/// filter state per channel. Output can exceed 1.0; callers normalize or limit it.
pub struct BassFilter {
    coeffs: [f64; 5],
    drive: f32,
    norm: f32,
    state: Vec<[f64; 4]>, // x1, x2, y1, y2 per channel
    channel: usize,
}

impl BassFilter {
    /// None when `strength` (clamped to 0..=1) is 0 or non-finite: nothing to do.
    pub fn new(channels: u16, sample_rate: u32, strength: f32) -> Option<Self> {
        let strength = if strength.is_finite() { strength.clamp(0.0, 1.0) } else { 0.0 };
        if strength == 0.0 {
            return None;
        }
        let drive = 1.0 + 3.0 * strength;
        Some(Self {
            coeffs: low_shelf(sample_rate.max(1) as f64, MAX_BOOST_DB * strength as f64),
            drive,
            norm: drive.tanh(),
            state: vec![[0.0; 4]; channels.max(1) as usize],
            channel: 0,
        })
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let c = &self.coeffs;
        let st = &mut self.state[self.channel];
        let x = sample as f64;
        let y = c[0] * x + c[1] * st[0] + c[2] * st[1] - c[3] * st[2] - c[4] * st[3];
        *st = [x, st[0], y, st[2]];
        self.channel = (self.channel + 1) % self.state.len();
        (self.drive * y as f32).tanh() / self.norm
    }
}

/// RBJ cookbook low shelf (slope 1), normalized: [b0, b1, b2, a1, a2].
fn low_shelf(rate: f64, gain_db: f64) -> [f64; 5] {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * SHELF_HZ / rate;
    let (sin, cos) = w0.sin_cos();
    let alpha = sin / 2.0 * std::f64::consts::SQRT_2;
    let k = 2.0 * a.sqrt() * alpha;
    let b0 = a * ((a + 1.0) - (a - 1.0) * cos + k);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos - k);
    let a0 = (a + 1.0) + (a - 1.0) * cos + k;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos);
    let a2 = (a + 1.0) + (a - 1.0) * cos - k;
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::audio::backend::to_f32;

    fn sine(freq: f32, amp: f32, channels: u16) -> DecodedBuffer {
        let rate = 44100;
        let samples = (0..rate).flat_map(|i| {
            let s = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
            std::iter::repeat(s).take(channels as usize)
        }).collect();
        DecodedBuffer::new(channels, rate as u32, samples)
    }

    fn rms(b: &DecodedBuffer) -> f32 {
        // skip the first 0.1 s so the filter has settled
        let skip = b.sample_rate as usize / 10 * b.channels as usize;
        let s = &b.samples[skip..];
        (s.iter().map(|&x| to_f32(x).powi(2)).sum::<f32>() / s.len() as f32).sqrt()
    }

    #[test]
    fn filter_matches_the_old_buffer_loop() {
        let b = sine(70.0, 0.8, 2);
        let strength = 0.7f32;
        // The loop bass_boost ran before BassFilter existed, without the normalization.
        let coeffs = low_shelf(44100.0, MAX_BOOST_DB * strength as f64);
        let drive = 1.0 + 3.0 * strength;
        let norm = drive.tanh();
        let mut state = vec![[0.0f64; 4]; 2];
        let old: Vec<f32> = b.samples.iter().enumerate().map(|(i, &x)| {
            let st = &mut state[i % 2];
            let x = to_f32(x) as f64;
            let y = coeffs[0] * x + coeffs[1] * st[0] + coeffs[2] * st[1] - coeffs[3] * st[2] - coeffs[4] * st[3];
            *st = [x, st[0], y, st[2]];
            (drive * y as f32).tanh() / norm
        }).collect();
        let mut f = BassFilter::new(2, 44100, strength).unwrap();
        let new: Vec<f32> = b.samples.iter().map(|&x| f.process(to_f32(x))).collect();
        assert_eq!(new, old);
        assert!(BassFilter::new(2, 44100, 0.0).is_none());
        assert!(BassFilter::new(2, 44100, f32::NAN).is_none());
    }

    #[test]
    fn zero_strength_shares_the_samples() {
        let b = sine(50.0, 0.1, 1);
        assert!(Arc::ptr_eq(&bass_boost(&b, 0.0).samples, &b.samples));
        assert!(Arc::ptr_eq(&bass_boost(&b, f32::NAN).samples, &b.samples));
    }

    #[test]
    fn boosts_lows_far_more_than_highs() {
        let (low, high) = (sine(50.0, 0.1, 1), sine(5000.0, 0.1, 1));
        let low_gain = rms(&bass_boost(&low, 1.0)) / rms(&low);
        let high_gain = rms(&bass_boost(&high, 1.0)) / rms(&high);
        assert!(low_gain > 2.0 * high_gain, "low {low_gain} high {high_gain}");
    }

    #[test]
    fn never_exceeds_full_scale() {
        for strength in [0.3, 1.0] {
            let out = bass_boost(&sine(60.0, 1.0, 2), strength);
            assert!(out.samples.iter().all(|&s| to_f32(s).abs() <= 1.0));
        }
    }

    #[test]
    fn keeps_format_and_filters_channels_independently() {
        let mut samples = Vec::new();
        for i in 0..44100 {
            let t = i as f32 / 44100.0;
            samples.push(0.1 * (2.0 * std::f32::consts::PI * 50.0 * t).sin()); // left: bass
            samples.push(0.0); // right: silence
        }
        let out = bass_boost(&DecodedBuffer::new(2, 44100, samples), 1.0);
        assert_eq!((out.channels, out.sample_rate, out.samples.len()), (2, 44100, 88200));
        assert!(out.samples.iter().skip(1).step_by(2).all(|&s| s == 0), "silent right stays silent");
    }
}
