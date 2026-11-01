//! Non-destructive trimming: slicing a decoded buffer to a play range, and waveform peaks.

use std::sync::Arc;

use super::backend::DecodedBuffer;
use crate::config::Trim;

/// Whole-frame range `[start, end)` that `trim` selects in `buffer`, clamped to the buffer.
/// None when the trim keeps everything, or when the clamped range would be empty (a trim that
/// no longer fits the file plays the whole file rather than nothing).
pub fn trim_frames(buffer: &DecodedBuffer, trim: Option<Trim>) -> Option<(usize, usize)> {
    let trim = trim?;
    let channels = buffer.channels.max(1) as usize;
    let frames = buffer.samples.len() / channels;
    let rate = buffer.sample_rate.max(1) as f64;
    let to_frame = |secs: f32| {
        let f = (secs.max(0.0) as f64 * rate).round();
        if f.is_finite() { (f as usize).min(frames) } else { frames }
    };
    let (start, end) = (to_frame(trim.start_secs), to_frame(trim.end_secs));
    if end <= start || (start == 0 && end == frames) {
        return None;
    }
    Some((start, end))
}

/// The part of `buffer` that `trim` keeps. Shares the input's samples (no copy) when `trim`
/// is None or covers the whole buffer.
pub fn apply_trim(buffer: &DecodedBuffer, trim: Option<Trim>) -> DecodedBuffer {
    match trim_frames(buffer, trim) {
        None => buffer.clone(),
        Some((start, end)) => {
            let channels = buffer.channels.max(1) as usize;
            let samples = buffer.samples[start * channels..end * channels].to_vec();
            DecodedBuffer { channels: buffer.channels, sample_rate: buffer.sample_rate, samples: Arc::new(samples) }
        }
    }
}

/// Length in seconds of what `trim` keeps of `buffer`, without slicing.
pub fn trimmed_duration(buffer: &DecodedBuffer, trim: Option<Trim>) -> f32 {
    match trim_frames(buffer, trim) {
        None => buffer.duration_secs(),
        Some((start, end)) => (end - start) as f32 / buffer.sample_rate.max(1) as f32,
    }
}

/// `buckets` peaks: per bucket the largest absolute sample over all channels, normalized so the
/// loudest bucket is 1.0. Silence stays all zeros; buckets past a very short clip's end are 0.
pub fn waveform(buffer: &DecodedBuffer, buckets: usize) -> Vec<f32> {
    let channels = buffer.channels.max(1) as usize;
    let frames = buffer.samples.len() / channels;
    let mut peaks = vec![0.0f32; buckets];
    if buckets == 0 || frames == 0 {
        return peaks;
    }
    for (i, peak) in peaks.iter_mut().enumerate() {
        let from = i * frames / buckets;
        let to = (i + 1) * frames / buckets;
        *peak = buffer.samples[from * channels..to * channels]
            .iter()
            .fold(0.0f32, |m, s| if s.is_finite() { m.max(s.abs()) } else { m });
    }
    let max = peaks.iter().copied().fold(0.0f32, f32::max);
    if max > 0.0 {
        for p in &mut peaks {
            *p /= max;
        }
    }
    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trim(start_secs: f32, end_secs: f32) -> Option<Trim> {
        Some(Trim { start_secs, end_secs })
    }

    /// Stereo at 10 Hz: frame i holds samples (i, -i).
    fn stereo(frames: usize) -> DecodedBuffer {
        DecodedBuffer::new(2, 10, (0..frames).flat_map(|i| [i as f32, -(i as f32)]).collect())
    }

    #[test]
    fn slices_whole_stereo_frames() {
        let b = stereo(20);
        let t = apply_trim(&b, trim(0.3, 0.8));
        assert_eq!(t.channels, 2);
        assert_eq!(t.sample_rate, 10);
        assert_eq!(*t.samples, vec![3.0, -3.0, 4.0, -4.0, 5.0, -5.0, 6.0, -6.0, 7.0, -7.0]);
        assert!((t.duration_secs() - 0.5).abs() < 1e-6);
        assert!((trimmed_duration(&b, trim(0.3, 0.8)) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn end_past_the_buffer_is_clamped() {
        let b = stereo(20);
        let t = apply_trim(&b, trim(1.5, 9.0));
        assert_eq!(t.samples.len(), 10, "frames 15..20");
        assert_eq!(t.samples[0], 15.0);
        assert_eq!(t.samples[9], -19.0);
    }

    #[test]
    fn none_and_full_range_share_the_samples() {
        let b = stereo(20);
        assert!(Arc::ptr_eq(&apply_trim(&b, None).samples, &b.samples));
        assert!(Arc::ptr_eq(&apply_trim(&b, trim(0.0, 2.0)).samples, &b.samples));
        assert!(Arc::ptr_eq(&apply_trim(&b, trim(0.0, 30.0)).samples, &b.samples));
        assert_eq!(trimmed_duration(&b, None), b.duration_secs());
    }

    #[test]
    fn trim_beyond_the_file_plays_everything() {
        let b = stereo(20);
        assert!(Arc::ptr_eq(&apply_trim(&b, trim(5.0, 6.0)).samples, &b.samples));
    }

    #[test]
    fn single_loud_frame_gives_one_full_bucket() {
        let mut samples = vec![0.0f32; 2 * 100];
        samples[2 * 42 + 1] = -0.5; // right channel of frame 42
        let peaks = waveform(&DecodedBuffer::new(2, 44100, samples), 10);
        assert_eq!(peaks.len(), 10);
        assert_eq!(peaks[4], 1.0);
        assert_eq!(peaks.iter().filter(|p| **p != 0.0).count(), 1);
    }

    #[test]
    fn silence_stays_zero_and_short_clips_fill_what_they_can() {
        assert_eq!(waveform(&DecodedBuffer::new(1, 44100, vec![0.0; 50]), 4), vec![0.0; 4]);
        let peaks = waveform(&DecodedBuffer::new(1, 44100, vec![0.25, 0.5]), 4);
        assert_eq!(peaks.len(), 4);
        assert_eq!(peaks.iter().copied().fold(0.0f32, f32::max), 1.0);
        assert!(waveform(&DecodedBuffer::new(1, 44100, vec![]), 0).is_empty());
    }
}
