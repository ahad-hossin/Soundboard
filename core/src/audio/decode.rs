use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rodio::{Decoder, Source};

use super::backend::{AudioError, DecodedBuffer};

pub const SUPPORTED_EXTENSIONS: [&str; 4] = ["mp3", "wav", "ogg", "flac"];

pub fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Fully decodes an audio file into interleaved f32 samples.
pub fn decode_file(path: &Path) -> Result<DecodedBuffer, AudioError> {
    let file = File::open(path)?;
    let decoder = Decoder::new(BufReader::new(file))
        .map_err(|e| AudioError::Decode(format!("{}: {e}", path.display())))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();
    if samples.is_empty() {
        return Err(AudioError::Decode(format!("{}: no audio data", path.display())));
    }
    Ok(DecodedBuffer::new(channels, sample_rate, samples))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::write_test_wav;
    use tempfile::tempdir;

    #[test]
    fn decodes_wav_to_expected_sample_count() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("clip.wav");
        write_test_wav(&p, 1000);
        let b = decode_file(&p).unwrap();
        assert_eq!(b.channels, 2);
        assert_eq!(b.sample_rate, 44100);
        assert_eq!(b.samples.len(), 2000);
        assert!((b.duration_secs() - 1000.0 / 44100.0).abs() < 1e-6);
    }

    #[test]
    fn garbage_file_is_decode_error() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("bad.wav");
        std::fs::write(&p, b"this is not audio at all").unwrap();
        assert!(matches!(decode_file(&p), Err(AudioError::Decode(_))));
    }

    #[test]
    fn missing_file_is_io_error() {
        assert!(matches!(decode_file(Path::new("Z:/nope/missing.wav")), Err(AudioError::Io(_))));
    }

    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(has_supported_extension(Path::new("a.MP3")));
        assert!(has_supported_extension(Path::new("a.flac")));
        assert!(!has_supported_extension(Path::new("a.txt")));
        assert!(!has_supported_extension(Path::new("noext")));
    }
}
