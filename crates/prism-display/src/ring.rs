// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cache-line padding to prevent false sharing between producer and consumer.
#[repr(align(64))]
struct CacheAligned(AtomicUsize);

impl CacheAligned {
    fn new(v: usize) -> Self {
        Self(AtomicUsize::new(v))
    }
}

/// Lock-free SPSC ring buffer for the capture→classify handoff.
///
/// # Safety
/// Only a single producer may call `try_push` and only a single consumer may
/// call `try_pop` at any time. Mixing callers breaks the SPSC invariant.
pub struct FrameRing<T> {
    slots: Box<[UnsafeCell<Option<T>>]>,
    capacity: usize,
    /// Producer-owned: only the producer writes this; consumer reads it.
    write_pos: CacheAligned,
    /// Consumer-owned: only the consumer writes this; producer reads it.
    read_pos: CacheAligned,
}

// SAFETY: SPSC invariant — only one producer calls try_push, one consumer
// calls try_pop. Atomic indices with Acquire/Release ordering prevent data
// races on the slot contents.
unsafe impl<T: Send> Send for FrameRing<T> {}
unsafe impl<T: Send> Sync for FrameRing<T> {}

impl<T> FrameRing<T> {
    /// Create a new `FrameRing` with `capacity` pre-allocated empty slots.
    ///
    /// # Panics
    /// Panics if `capacity` is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "FrameRing capacity must be > 0");
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(UnsafeCell::new(None));
        }
        Self {
            slots: slots.into_boxed_slice(),
            capacity,
            write_pos: CacheAligned::new(0),
            read_pos: CacheAligned::new(0),
        }
    }

    /// Push an item onto the ring.  Returns `Some(())` on success, `None` when
    /// the ring is full.
    ///
    /// # Safety (caller contract)
    /// Must be called from exactly one producer thread.
    pub fn try_push(&self, item: T) -> Option<()> {
        let write = self.write_pos.0.load(Ordering::Relaxed);
        // Acquire: observe the consumer's most recent read_pos update so we
        // don't overwrite a slot the consumer hasn't finished with.
        let read = self.read_pos.0.load(Ordering::Acquire);

        if write.wrapping_sub(read) >= self.capacity {
            return None; // full
        }

        let slot = write % self.capacity;
        // SAFETY: we just verified the slot is vacant (producer owns it).
        // No other thread writes to this index right now (SPSC invariant).
        unsafe {
            *self.slots[slot].get() = Some(item);
        }

        // Release: publish the written data to the consumer.
        self.write_pos
            .0
            .store(write.wrapping_add(1), Ordering::Release);
        Some(())
    }

    /// Pop an item from the ring.  Returns `Some(T)` when an item is
    /// available, `None` when the ring is empty.
    ///
    /// # Safety (caller contract)
    /// Must be called from exactly one consumer thread.
    pub fn try_pop(&self) -> Option<T> {
        let read = self.read_pos.0.load(Ordering::Relaxed);
        // Acquire: observe the producer's most recent write_pos update.
        let write = self.write_pos.0.load(Ordering::Acquire);

        if read == write {
            return None; // empty
        }

        let slot = read % self.capacity;
        // SAFETY: we just verified the slot is occupied (consumer owns it).
        // The producer will not touch this index until we advance read_pos.
        let item = unsafe { (*self.slots[slot].get()).take() };

        // Release: publish the freed slot to the producer.
        self.read_pos
            .0
            .store(read.wrapping_add(1), Ordering::Release);

        item
    }

    /// Number of items currently in the ring.  Approximate under concurrency.
    pub fn len(&self) -> usize {
        let write = self.write_pos.0.load(Ordering::Relaxed);
        let read = self.read_pos.0.load(Ordering::Relaxed);
        write.wrapping_sub(read)
    }

    /// Returns `true` if the ring contains no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the ring is at capacity.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // 1. Basic push then pop.
    #[test]
    fn push_pop_single() {
        let ring: FrameRing<i32> = FrameRing::new(4);
        assert_eq!(ring.try_push(42), Some(()));
        assert_eq!(ring.try_pop(), Some(42));
    }

    // 2. Pop from empty ring returns None.
    #[test]
    fn empty_pop_returns_none() {
        let ring: FrameRing<i32> = FrameRing::new(4);
        assert_eq!(ring.try_pop(), None);
    }

    // 3. A full ring rejects further pushes.
    #[test]
    fn full_ring_rejects_push() {
        let ring: FrameRing<i32> = FrameRing::new(2);
        assert_eq!(ring.try_push(1), Some(()));
        assert_eq!(ring.try_push(2), Some(()));
        // Ring is now full; third push must fail.
        assert_eq!(ring.try_push(3), None);
        // The two original items are still there.
        assert_eq!(ring.try_pop(), Some(1));
        assert_eq!(ring.try_pop(), Some(2));
    }

    // 4. Items come out in FIFO order.
    #[test]
    fn fifo_ordering() {
        let ring: FrameRing<i32> = FrameRing::new(8);
        for i in 1..=3 {
            ring.try_push(i).unwrap();
        }
        assert_eq!(ring.try_pop(), Some(1));
        assert_eq!(ring.try_pop(), Some(2));
        assert_eq!(ring.try_pop(), Some(3));
        assert_eq!(ring.try_pop(), None);
    }

    // 5. Ring wraps around correctly (index overflow path).
    #[test]
    fn wraparound() {
        let ring: FrameRing<i32> = FrameRing::new(2);
        // Fill, drain one, refill — exercises the slot index wrap.
        ring.try_push(10).unwrap();
        ring.try_push(20).unwrap();
        assert_eq!(ring.try_pop(), Some(10)); // slot 0 freed
        ring.try_push(30).unwrap(); // reuse slot 0
        assert_eq!(ring.try_pop(), Some(20));
        assert_eq!(ring.try_pop(), Some(30));
        assert_eq!(ring.try_pop(), None);
    }

    // 6. len / is_empty / is_full track state correctly.
    #[test]
    fn len_tracking() {
        let ring: FrameRing<i32> = FrameRing::new(3);
        assert!(ring.is_empty());
        assert!(!ring.is_full());
        assert_eq!(ring.len(), 0);

        ring.try_push(1).unwrap();
        assert_eq!(ring.len(), 1);
        assert!(!ring.is_empty());
        assert!(!ring.is_full());

        ring.try_push(2).unwrap();
        ring.try_push(3).unwrap();
        assert_eq!(ring.len(), 3);
        assert!(!ring.is_empty());
        assert!(ring.is_full());

        ring.try_pop();
        assert_eq!(ring.len(), 2);
        assert!(!ring.is_full());
    }

    // 7. Concurrent producer/consumer: all 1000 items arrive in order.
    #[test]
    fn concurrent_producer_consumer() {
        const N: usize = 1_000;
        const CAPACITY: usize = 64;

        let ring = Arc::new(FrameRing::<usize>::new(CAPACITY));

        let producer_ring = Arc::clone(&ring);
        let producer = std::thread::spawn(move || {
            for i in 0..N {
                // Spin until space is available.
                loop {
                    if producer_ring.try_push(i).is_some() {
                        break;
                    }
                    std::hint::spin_loop();
                }
            }
        });

        let consumer_ring = Arc::clone(&ring);
        let consumer = std::thread::spawn(move || {
            let mut received = Vec::with_capacity(N);
            while received.len() < N {
                if let Some(v) = consumer_ring.try_pop() {
                    received.push(v);
                } else {
                    std::hint::spin_loop();
                }
            }
            received
        });

        producer.join().expect("producer panicked");
        let received = consumer.join().expect("consumer panicked");

        assert_eq!(
            received.len(),
            N,
            "expected {N} items, got {}",
            received.len()
        );
        for (idx, &val) in received.iter().enumerate() {
            assert_eq!(val, idx, "FIFO violation at position {idx}: got {val}");
        }
    }
}
