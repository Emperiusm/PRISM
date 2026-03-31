// Active probing for transport quality measurement.

use std::collections::HashMap;
use std::time::{Duration, Instant};

// ── ActivityState ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    ActiveStreaming,
    ActiveTransfer,
    BackgroundSync,
    Idle,
}

impl ActivityState {
    fn probe_interval(self) -> Duration {
        match self {
            ActivityState::ActiveStreaming => Duration::from_secs(2),
            ActivityState::ActiveTransfer  => Duration::from_secs(5),
            ActivityState::BackgroundSync  => Duration::from_secs(30),
            ActivityState::Idle            => Duration::from_secs(60),
        }
    }
}

// ── Wire types ────────────────────────────────────────────────────────────────

/// 12-byte probe payload: [seq: u32 LE][sender_timestamp_us: u64 LE]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbePayload {
    pub seq: u32,
    pub sender_timestamp_us: u64,
}

impl ProbePayload {
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0..4].copy_from_slice(&self.seq.to_le_bytes());
        b[4..12].copy_from_slice(&self.sender_timestamp_us.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 12 {
            return None;
        }
        let seq = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let sender_timestamp_us = u64::from_le_bytes([b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11]]);
        Some(Self { seq, sender_timestamp_us })
    }
}

/// 20-byte probe echo: [seq: u32 LE][sender_timestamp_us: u64 LE][responder_timestamp_us: u64 LE]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeEcho {
    pub seq: u32,
    pub sender_timestamp_us: u64,
    pub responder_timestamp_us: u64,
}

impl ProbeEcho {
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut b = [0u8; 20];
        b[0..4].copy_from_slice(&self.seq.to_le_bytes());
        b[4..12].copy_from_slice(&self.sender_timestamp_us.to_le_bytes());
        b[12..20].copy_from_slice(&self.responder_timestamp_us.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 20 {
            return None;
        }
        let seq = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let sender_timestamp_us = u64::from_le_bytes([b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11]]);
        let responder_timestamp_us = u64::from_le_bytes([b[12], b[13], b[14], b[15], b[16], b[17], b[18], b[19]]);
        Some(Self { seq, sender_timestamp_us, responder_timestamp_us })
    }
}

// ── ProbeResult ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ProbeResult {
    pub rtt: Duration,
    pub local_send_us: u64,
    pub local_recv_us: u64,
    pub remote_timestamp_us: u64,
}

// ── ConnectionProber ──────────────────────────────────────────────────────────

pub struct ConnectionProber {
    epoch: Instant,
    pending: HashMap<u32, Instant>,
    next_seq: u32,
    interval: Duration,
    last_probe: Option<Instant>,
    rtt: Option<Duration>,
}

impl ConnectionProber {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            pending: HashMap::new(),
            next_seq: 0,
            interval: ActivityState::ActiveStreaming.probe_interval(),
            last_probe: None,
            rtt: None,
        }
    }

    pub fn probe_interval(&self) -> Duration {
        self.interval
    }

    pub fn set_activity(&mut self, state: ActivityState) {
        self.interval = state.probe_interval();
    }

    pub fn latest_rtt(&self) -> Option<Duration> {
        self.rtt
    }

    /// Generate a probe if the interval has elapsed (or this is the very first probe).
    pub fn generate_probe(&mut self) -> Option<ProbePayload> {
        let now = Instant::now();
        if let Some(last) = self.last_probe {
            if now.duration_since(last) < self.interval {
                return None;
            }
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        let sender_timestamp_us = now.duration_since(self.epoch).as_micros() as u64;
        self.pending.insert(seq, now);
        self.last_probe = Some(now);
        Some(ProbePayload { seq, sender_timestamp_us })
    }

    /// Process an echo, compute RTT, return ProbeResult if seq was pending.
    pub fn process_echo(&mut self, echo: &ProbeEcho, recv_time: Instant) -> Option<ProbeResult> {
        let send_time = self.pending.remove(&echo.seq)?;
        let rtt = recv_time.duration_since(send_time);
        self.rtt = Some(rtt);
        let local_send_us = send_time.duration_since(self.epoch).as_micros() as u64;
        let local_recv_us = recv_time.duration_since(self.epoch).as_micros() as u64;
        Some(ProbeResult {
            rtt,
            local_send_us,
            local_recv_us,
            remote_timestamp_us: echo.responder_timestamp_us,
        })
    }
}

impl Default for ConnectionProber {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_prober_generates_first_probe_immediately() {
        let mut p = ConnectionProber::new();
        let probe = p.generate_probe();
        assert!(probe.is_some());
        assert_eq!(probe.unwrap().seq, 0);
    }

    #[test]
    fn second_probe_respects_interval() {
        let mut p = ConnectionProber::new();
        p.generate_probe();
        assert!(p.generate_probe().is_none());
    }

    #[test]
    fn process_echo_computes_rtt() {
        let mut p = ConnectionProber::new();
        let probe = p.generate_probe().unwrap();
        let echo = ProbeEcho {
            seq: probe.seq,
            sender_timestamp_us: probe.sender_timestamp_us,
            responder_timestamp_us: 999_999,
        };
        std::thread::sleep(Duration::from_millis(1));
        let result = p.process_echo(&echo, Instant::now());
        assert!(result.is_some());
        assert!(result.unwrap().rtt >= Duration::from_millis(1));
    }

    #[test]
    fn process_unknown_echo_returns_none() {
        let mut p = ConnectionProber::new();
        let echo = ProbeEcho { seq: 999, sender_timestamp_us: 0, responder_timestamp_us: 0 };
        assert!(p.process_echo(&echo, Instant::now()).is_none());
    }

    #[test]
    fn adaptive_frequency_changes_interval() {
        let mut p = ConnectionProber::new();
        p.set_activity(ActivityState::Idle);
        assert_eq!(p.probe_interval(), Duration::from_secs(60));
        p.set_activity(ActivityState::ActiveStreaming);
        assert_eq!(p.probe_interval(), Duration::from_secs(2));
    }

    #[test]
    fn probe_payload_roundtrip() {
        let pl = ProbePayload { seq: 42, sender_timestamp_us: 123_456_789 };
        let b = pl.to_bytes();
        let d = ProbePayload::from_bytes(&b).unwrap();
        assert_eq!(d.seq, 42);
        assert_eq!(d.sender_timestamp_us, 123_456_789);
    }

    #[test]
    fn probe_echo_roundtrip() {
        let echo = ProbeEcho { seq: 7, sender_timestamp_us: 100, responder_timestamp_us: 200 };
        let b = echo.to_bytes();
        let d = ProbeEcho::from_bytes(&b).unwrap();
        assert_eq!(d.seq, 7);
        assert_eq!(d.sender_timestamp_us, 100);
        assert_eq!(d.responder_timestamp_us, 200);
    }

    #[test]
    fn latest_rtt_updates() {
        let mut p = ConnectionProber::new();
        assert!(p.latest_rtt().is_none());
        let probe = p.generate_probe().unwrap();
        let echo = ProbeEcho {
            seq: probe.seq,
            sender_timestamp_us: probe.sender_timestamp_us,
            responder_timestamp_us: 0,
        };
        std::thread::sleep(Duration::from_millis(1));
        p.process_echo(&echo, Instant::now());
        assert!(p.latest_rtt().is_some());
    }
}
