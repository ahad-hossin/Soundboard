pub mod backend;
pub mod cache;
pub mod decode;
pub mod engine;
pub mod fake_backend;
pub mod trim;

pub use backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};
pub use cache::BufferCache;
pub use engine::{sine, Engine, Outputs, PlayOutcome, PREVIEW_ID, TEST_TONE_ID};
pub use fake_backend::FakeBackend;
pub use trim::{apply_trim, waveform};
pub mod rodio_backend;
pub use rodio_backend::RodioBackend;
