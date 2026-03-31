// Packet coalescing: batch small packets into larger sends.

use bytes::BytesMut;
use std::time::{Duration, Instant};

const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(5);

pub struct DatagramCoalescer {
    buffer: BytesMut,
    max_size: usize,
    flush_interval: Duration,
    last_flush: Instant,
}

impl DatagramCoalescer {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: BytesMut::new(),
            max_size,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            last_flush: Instant::now(),
        }
    }

    pub fn with_flush_interval(max_size: usize, interval: Duration) -> Self {
        Self {
            buffer: BytesMut::new(),
            max_size,
            flush_interval: interval,
            last_flush: Instant::now(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Append a length-prefixed message into the coalesce buffer.
    pub fn push(&mut self, data: &[u8]) {
        let len = data.len() as u32;
        self.buffer.extend_from_slice(&len.to_le_bytes());
        self.buffer.extend_from_slice(data);
    }

    /// Return the accumulated buffer as a `Vec<u8>` and reset internal state.
    pub fn flush(&mut self) -> Vec<u8> {
        self.last_flush = Instant::now();
        std::mem::take(&mut self.buffer).to_vec()
    }

    /// Returns true when the buffer has grown past max_size or the flush interval elapsed.
    pub fn should_flush(&self) -> bool {
        self.buffer.len() >= self.max_size || self.last_flush.elapsed() >= self.flush_interval
    }

    /// Parse a coalesced datagram into its constituent messages.
    pub fn split(data: &[u8]) -> Vec<&[u8]> {
        let mut msgs = Vec::new();
        let mut pos = 0;
        while pos + 4 <= data.len() {
            let len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
            pos += 4;
            if pos + len <= data.len() {
                msgs.push(&data[pos..pos + len]);
            }
            pos += len;
        }
        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_coalescer_produces_nothing() {
        let c = DatagramCoalescer::new(1200);
        assert!(c.is_empty());
    }

    #[test]
    fn push_and_flush_single() {
        let mut c = DatagramCoalescer::new(1200);
        c.push(b"hello");
        assert!(!c.is_empty());
        let f = c.flush();
        assert_eq!(&f[0..4], &5u32.to_le_bytes());
        assert_eq!(&f[4..9], b"hello");
        assert!(c.is_empty());
    }

    #[test]
    fn coalesce_multiple_small_messages() {
        let mut c = DatagramCoalescer::new(1200);
        c.push(b"aa");
        c.push(b"bb");
        c.push(b"cc");
        let f = c.flush();
        assert_eq!(f.len(), (4 + 2) * 3);
    }

    #[test]
    fn should_flush_when_exceeds_max_size() {
        let mut c = DatagramCoalescer::new(20);
        c.push(b"aaaaaaaaaa");
        assert!(!c.should_flush());
        c.push(b"bbbbbbbbbb");
        assert!(c.should_flush());
    }

    #[test]
    fn should_flush_after_interval() {
        let mut c = DatagramCoalescer::with_flush_interval(1200, Duration::from_millis(1));
        c.push(b"data");
        std::thread::sleep(Duration::from_millis(2));
        assert!(c.should_flush());
    }

    #[test]
    fn split_coalesced_datagram() {
        let mut c = DatagramCoalescer::new(1200);
        c.push(b"first");
        c.push(b"second");
        let f = c.flush();
        let msgs = DatagramCoalescer::split(&f);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], b"first");
        assert_eq!(msgs[1], b"second");
    }
}
