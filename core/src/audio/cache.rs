use std::collections::{HashMap, VecDeque};

use super::backend::DecodedBuffer;

/// LRU cache of decoded buffers keyed by sound id, bounded by total bytes.
/// Always keeps at least one entry so a single oversized clip still plays.
pub struct BufferCache {
    max_bytes: usize,
    total: usize,
    entries: HashMap<String, DecodedBuffer>,
    order: VecDeque<String>,
}

impl BufferCache {
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes, total: 0, entries: HashMap::new(), order: VecDeque::new() }
    }

    pub fn get(&mut self, id: &str) -> Option<DecodedBuffer> {
        let buf = self.entries.get(id)?.clone();
        self.touch(id);
        Some(buf)
    }

    pub fn insert(&mut self, id: &str, buf: DecodedBuffer) {
        self.remove(id);
        self.total += buf.byte_len();
        self.entries.insert(id.to_string(), buf);
        self.order.push_back(id.to_string());
        while self.total > self.max_bytes && self.order.len() > 1 {
            if let Some(old) = self.order.pop_front() {
                if let Some(b) = self.entries.remove(&old) {
                    self.total -= b.byte_len();
                }
            }
        }
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(b) = self.entries.remove(id) {
            self.total -= b.byte_len();
            self.order.retain(|k| k != id);
        }
    }

    /// Duration of a cached clip without touching its LRU position.
    pub fn duration_secs(&self, id: &str) -> Option<f32> {
        self.entries.get(id).map(|b| b.duration_secs())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    /// True when `bytes` more would stay within the cap without evicting anything.
    pub fn has_room_for(&self, bytes: usize) -> bool {
        self.total + bytes <= self.max_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.total
    }

    fn touch(&mut self, id: &str) {
        self.order.retain(|k| k != id);
        self.order.push_back(id.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(n: usize) -> DecodedBuffer {
        DecodedBuffer::new(1, 44100, vec![0.5; n])
    }

    #[test]
    fn insert_and_get_returns_buffer() {
        let mut c = BufferCache::new(1024);
        c.insert("a", buf(4));
        let got = c.get("a").unwrap();
        assert_eq!(got.samples.len(), 4);
        assert_eq!(c.total_bytes(), 16);
    }

    #[test]
    fn evicts_least_recently_used_when_over_cap() {
        // cap = 16 bytes = four f32 samples; each buffer is 2 samples = 8 bytes
        let mut c = BufferCache::new(16);
        c.insert("a", buf(2));
        c.insert("b", buf(2));
        assert!(c.get("a").is_some(), "touch a so b is the oldest");
        c.insert("c", buf(2));
        assert!(c.get("b").is_none(), "b evicted");
        assert!(c.get("a").is_some());
        assert!(c.get("c").is_some());
        assert_eq!(c.len(), 2);
        assert_eq!(c.total_bytes(), 16);
    }

    #[test]
    fn remove_frees_bytes() {
        let mut c = BufferCache::new(1024);
        c.insert("a", buf(4));
        c.remove("a");
        assert!(c.is_empty());
        assert_eq!(c.total_bytes(), 0);
    }

    #[test]
    fn single_oversized_entry_is_kept() {
        let mut c = BufferCache::new(8);
        c.insert("big", buf(100));
        assert!(c.get("big").is_some());
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn contains_and_room() {
        let mut c = BufferCache::new(16);
        assert!(c.has_room_for(16));
        c.insert("a", buf(2));
        assert!(c.contains("a"));
        assert!(!c.contains("b"));
        assert!(c.has_room_for(8));
        assert!(!c.has_room_for(9));
    }

    #[test]
    fn reinserting_same_id_replaces() {
        let mut c = BufferCache::new(1024);
        c.insert("a", buf(2));
        c.insert("a", buf(6));
        assert_eq!(c.len(), 1);
        assert_eq!(c.total_bytes(), 24);
    }
}
