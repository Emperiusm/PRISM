// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

// Stream pool: manage reusable QUIC streams per channel.

use crate::connection::{
    OwnedRecvStream, OwnedSendStream, PrismConnection, StreamPriority, TransportError,
};

pub const DEFAULT_POOL_SIZE: usize = 4;

pub struct StreamPool {
    pool: Vec<(OwnedSendStream, OwnedRecvStream)>,
    pool_size: usize,
}

impl StreamPool {
    pub fn new(pool_size: usize) -> Self {
        Self {
            pool: Vec::with_capacity(pool_size),
            pool_size,
        }
    }

    pub fn available(&self) -> usize {
        self.pool.len()
    }

    /// Take a stream from the pool, or open a new one if the pool is empty.
    pub async fn acquire(
        &mut self,
        conn: &dyn PrismConnection,
        priority: StreamPriority,
    ) -> Result<(OwnedSendStream, OwnedRecvStream), TransportError> {
        if let Some(pair) = self.pool.pop() {
            return Ok(pair);
        }
        conn.open_bi(priority).await
    }

    /// Fill the pool up to `pool_size` by opening new streams.
    pub async fn maintain(&mut self, conn: &dyn PrismConnection, priority: StreamPriority) {
        while self.pool.len() < self.pool_size {
            match conn.open_bi(priority).await {
                Ok(pair) => self.pool.push(pair),
                Err(_) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::mock::MockConnection;

    #[tokio::test]
    async fn empty_pool_opens_new_stream() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(4);
        assert_eq!(pool.available(), 0);
        let (_s, _r) = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
    }

    #[tokio::test]
    async fn maintain_fills_pool() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(4);
        pool.maintain(&conn, StreamPriority::Low).await;
        assert_eq!(pool.available(), 4);
    }

    #[tokio::test]
    async fn acquire_from_maintained_pool() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(4);
        pool.maintain(&conn, StreamPriority::Low).await;
        let (_s, _r) = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
        assert_eq!(pool.available(), 3);
    }

    #[tokio::test]
    async fn acquire_drains_pool_then_opens_new() {
        let conn = MockConnection::new(1200);
        let mut pool = StreamPool::new(2);
        pool.maintain(&conn, StreamPriority::Low).await;
        let _ = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
        let _ = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
        assert_eq!(pool.available(), 0);
        let _ = pool.acquire(&conn, StreamPriority::Low).await.unwrap();
    }
}
