//! GPU texture pool management.
//!
//! Tracks pre-allocated texture slots through a state machine:
//! `Free → Writing → Ready → Reading → Free`.
//!
//! The pool is pure logic — no GPU handles are stored here. The caller owns
//! the actual GPU allocations and looks them up by `TextureSlot` index.

use prism_display::{SharedTexture, TextureFormat};

// ── TexturePoolConfig ─────────────────────────────────────────────────────────

/// Configuration parameters for a [`TexturePool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TexturePoolConfig {
    /// Width of each texture in the pool (pixels).
    pub width: u32,
    /// Height of each texture in the pool (pixels).
    pub height: u32,
    /// Pixel format of each texture.
    pub format: TextureFormat,
    /// Number of texture slots in the pool.
    pub pool_size: usize,
}

impl TexturePoolConfig {
    /// Sensible defaults for a full-desktop display capture:
    /// pool of 4 BGRA8 textures.
    pub fn for_display(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: TextureFormat::Bgra8,
            pool_size: 4,
        }
    }
}

// ── TextureSlot ───────────────────────────────────────────────────────────────

/// Index into a [`TexturePool`].  Cheap to copy and compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureSlot(pub usize);

// ── TextureSlotState ──────────────────────────────────────────────────────────

/// Internal state-machine state for a single pool slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextureSlotState {
    /// Available for a new capture write.
    Free,
    /// A capture is writing into this slot (not yet committed).
    Writing,
    /// Fully captured frame available for the encoder to read.
    Ready,
    /// The encoder is currently reading from this slot.
    Reading,
}

// ── TexturePool ───────────────────────────────────────────────────────────────

/// Double-buffered GPU texture state manager.
///
/// Callers use `acquire_write` / `commit_write` on the capture side and
/// `acquire_read` / `release_read` on the encode side.  At no point does
/// the pool hold GPU memory itself — it only tracks which logical slot is in
/// which state.
pub struct TexturePool {
    config: TexturePoolConfig,
    slots: Vec<TextureSlotState>,
}

impl TexturePool {
    /// Create a new pool according to `config`.  All slots start as `Free`.
    pub fn new(config: TexturePoolConfig) -> Self {
        Self {
            slots: vec![TextureSlotState::Free; config.pool_size],
            config,
        }
    }

    // ── Write side ────────────────────────────────────────────────────────────

    /// Find the first `Free` slot, mark it `Writing`, and return its index.
    ///
    /// Returns `None` when all slots are occupied.
    pub fn acquire_write(&mut self) -> Option<TextureSlot> {
        self.slots.iter().position(|s| *s == TextureSlotState::Free).map(|i| {
            self.slots[i] = TextureSlotState::Writing;
            TextureSlot(i)
        })
    }

    /// Transition `slot` from `Writing` to `Ready`.
    ///
    /// # Panics
    /// Panics if `slot` is not in the `Writing` state.
    pub fn commit_write(&mut self, slot: TextureSlot) {
        assert_eq!(
            self.slots[slot.0],
            TextureSlotState::Writing,
            "commit_write called on slot {} which is not in Writing state",
            slot.0
        );
        self.slots[slot.0] = TextureSlotState::Ready;
    }

    /// Abandon an in-progress write, returning `slot` to `Free`.
    ///
    /// Use this to roll back when a capture operation fails partway through.
    ///
    /// # Panics
    /// Panics if `slot` is not in the `Writing` state.
    pub fn abandon_write(&mut self, slot: TextureSlot) {
        assert_eq!(
            self.slots[slot.0],
            TextureSlotState::Writing,
            "abandon_write called on slot {} which is not in Writing state",
            slot.0
        );
        self.slots[slot.0] = TextureSlotState::Free;
    }

    // ── Read side ─────────────────────────────────────────────────────────────

    /// Find the first `Ready` slot, mark it `Reading`, and return its index.
    ///
    /// Returns `None` when no committed frame is available yet.
    pub fn acquire_read(&mut self) -> Option<TextureSlot> {
        self.slots.iter().position(|s| *s == TextureSlotState::Ready).map(|i| {
            self.slots[i] = TextureSlotState::Reading;
            TextureSlot(i)
        })
    }

    /// Transition `slot` from `Reading` back to `Free`.
    ///
    /// # Panics
    /// Panics if `slot` is not in the `Reading` state.
    pub fn release_read(&mut self, slot: TextureSlot) {
        assert_eq!(
            self.slots[slot.0],
            TextureSlotState::Reading,
            "release_read called on slot {} which is not in Reading state",
            slot.0
        );
        self.slots[slot.0] = TextureSlotState::Free;
    }

    // ── Shared-texture construction ───────────────────────────────────────────

    /// Build a [`SharedTexture`] descriptor for `slot` using the given
    /// platform-specific `handle` (e.g. a Windows NT HANDLE cast to `u64`).
    pub fn shared_texture(&self, slot: TextureSlot, handle: u64) -> SharedTexture {
        let _ = slot; // slot is the caller's index into their own GPU array
        SharedTexture {
            handle,
            width: self.config.width,
            height: self.config.height,
            format: self.config.format,
        }
    }

    // ── Diagnostics ───────────────────────────────────────────────────────────

    /// Number of `Free` slots.
    pub fn free_count(&self) -> usize {
        self.slots.iter().filter(|s| **s == TextureSlotState::Free).count()
    }

    /// Number of `Ready` slots (captured frames awaiting encode).
    pub fn ready_count(&self) -> usize {
        self.slots.iter().filter(|s| **s == TextureSlotState::Ready).count()
    }

    /// Total number of slots in this pool.
    pub fn pool_size(&self) -> usize {
        self.config.pool_size
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pool4() -> TexturePool {
        TexturePool::new(TexturePoolConfig::for_display(1920, 1080))
    }

    #[test]
    fn acquire_write_from_empty_pool() {
        let mut pool = pool4();
        assert_eq!(pool.free_count(), 4);
        let slot = pool.acquire_write().expect("should get a slot");
        assert_eq!(slot.0, 0);
        assert_eq!(pool.free_count(), 3);
    }

    #[test]
    fn commit_makes_slot_ready() {
        let mut pool = pool4();
        let slot = pool.acquire_write().unwrap();
        assert_eq!(pool.ready_count(), 0);
        pool.commit_write(slot);
        assert_eq!(pool.ready_count(), 1);
        assert_eq!(pool.free_count(), 3);
    }

    #[test]
    fn read_after_commit() {
        let mut pool = pool4();
        let w = pool.acquire_write().unwrap();
        pool.commit_write(w);
        let r = pool.acquire_read().expect("should have a ready slot");
        assert_eq!(r.0, 0);
        assert_eq!(pool.ready_count(), 0);
        assert_eq!(pool.free_count(), 3); // slot is now Reading, not Free
    }

    #[test]
    fn release_read_frees_slot() {
        let mut pool = pool4();
        let w = pool.acquire_write().unwrap();
        pool.commit_write(w);
        let r = pool.acquire_read().unwrap();
        assert_eq!(pool.free_count(), 3);
        pool.release_read(r);
        assert_eq!(pool.free_count(), 4);
    }

    #[test]
    fn pool_exhaustion_returns_none() {
        let mut pool = pool4();
        // Acquire all 4 slots for writing without committing.
        let s0 = pool.acquire_write().unwrap();
        let s1 = pool.acquire_write().unwrap();
        let s2 = pool.acquire_write().unwrap();
        let s3 = pool.acquire_write().unwrap();
        assert_ne!(s0.0, s1.0);
        assert_ne!(s2.0, s3.0);
        assert_eq!(pool.free_count(), 0);
        assert!(pool.acquire_write().is_none(), "pool should be exhausted");
    }

    #[test]
    fn abandon_write_returns_slot_to_free() {
        let mut pool = pool4();
        let slot = pool.acquire_write().unwrap();
        pool.abandon_write(slot);
        assert_eq!(pool.free_count(), 4);
        // Should be acquirable again.
        let slot2 = pool.acquire_write().unwrap();
        assert_eq!(slot2.0, 0);
    }

    #[test]
    fn double_buffer_flow() {
        // Simulate: write frame A, read A while writing frame B.
        let mut pool = pool4();

        let wa = pool.acquire_write().unwrap(); // slot 0 → Writing
        pool.commit_write(wa);                  // slot 0 → Ready

        let ra = pool.acquire_read().unwrap();  // slot 0 → Reading
        let wb = pool.acquire_write().unwrap(); // slot 1 → Writing
        pool.commit_write(wb);                  // slot 1 → Ready

        // Encoder finishes slot 0; capture can reuse it.
        // State: slot 0 → Free, slot 1 → Ready, slots 2-3 → Free
        pool.release_read(ra);                  // slot 0 → Free
        assert_eq!(pool.free_count(), 3);
        assert_eq!(pool.ready_count(), 1);

        // Encoder picks up slot 1.
        let rb = pool.acquire_read().unwrap();
        assert_eq!(rb.0, 1);
        pool.release_read(rb);
        assert_eq!(pool.free_count(), 4);
    }

    #[test]
    fn shared_texture_from_slot() {
        let pool = pool4();
        let slot = TextureSlot(2);
        let tex = pool.shared_texture(slot, 0xABCD_1234);
        assert_eq!(tex.handle, 0xABCD_1234);
        assert_eq!(tex.width, 1920);
        assert_eq!(tex.height, 1080);
        assert_eq!(tex.format, TextureFormat::Bgra8);
    }
}
