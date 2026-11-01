pub mod backend;
pub mod cache;
pub mod decode;
pub mod engine;
pub mod fake_backend;

pub use backend::{AudioBackend, AudioError, DecodedBuffer, DeviceInfo, DeviceTarget, SinkHandle};
pub use cache::BufferCache;
pub use engine::{sine, Engine, Outputs, PlayOutcome, TEST_TONE_ID};
pub use fake_backend::FakeBackend;
