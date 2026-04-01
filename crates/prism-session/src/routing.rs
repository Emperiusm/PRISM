// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::types::ClientId;

/// A single routing entry pointing to one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEntry {
    pub client_id: ClientId,
}

/// An immutable snapshot of the routing table at a point in time.
#[derive(Debug, Clone, Default)]
pub struct RoutingSnapshot {
    /// Maps channel_id -> ordered list of recipients.
    pub channel_routes: HashMap<u16, Vec<RouteEntry>>,
    /// Monotonically increasing; increments once per `batch_update` call.
    pub generation: u64,
}

/// Mutations that can be applied to the routing table in a batch.
#[derive(Debug, Clone)]
pub enum RoutingMutation {
    /// Add a route entry for a channel.
    AddRoute { channel_id: u16, entry: RouteEntry },
    /// Remove all routes that reference a specific client.
    RemoveClient(ClientId),
    /// Replace `from` with `to_entry` on `channel_id`, preserving all others.
    TransferChannel {
        channel_id: u16,
        from: ClientId,
        to_entry: RouteEntry,
    },
}

/// Lock-free routing table backed by ArcSwap for wait-free reads.
pub struct RoutingTable {
    inner: ArcSwap<RoutingSnapshot>,
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingTable {
    /// Create an empty routing table at generation 0.
    pub fn new() -> Self {
        Self {
            inner: ArcSwap::from_pointee(RoutingSnapshot::default()),
        }
    }

    /// Return the current snapshot. Callers hold an `Arc`; their view is
    /// stable even while concurrent writers publish updates.
    pub fn snapshot(&self) -> Arc<RoutingSnapshot> {
        self.inner.load_full()
    }

    /// Apply all mutations atomically (single swap) and increment generation.
    pub fn batch_update(&self, mutations: Vec<RoutingMutation>) {
        let current = self.inner.load_full();
        let mut next = (*current).clone();

        for mutation in mutations {
            match mutation {
                RoutingMutation::AddRoute { channel_id, entry } => {
                    next.channel_routes
                        .entry(channel_id)
                        .or_default()
                        .push(entry);
                }
                RoutingMutation::RemoveClient(client_id) => {
                    for routes in next.channel_routes.values_mut() {
                        routes.retain(|e| e.client_id != client_id);
                    }
                    // Remove channels that became empty
                    next.channel_routes.retain(|_, v| !v.is_empty());
                }
                RoutingMutation::TransferChannel {
                    channel_id,
                    from,
                    to_entry,
                } => {
                    if let Some(routes) = next.channel_routes.get_mut(&channel_id) {
                        for entry in routes.iter_mut() {
                            if entry.client_id == from {
                                *entry = to_entry.clone();
                                break;
                            }
                        }
                    }
                }
            }
        }

        next.generation = current.generation + 1;
        self.inner.store(Arc::new(next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn cid() -> ClientId {
        Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext))
    }

    fn entry(client_id: ClientId) -> RouteEntry {
        RouteEntry { client_id }
    }

    #[test]
    fn empty_table_generation_zero() {
        let table = RoutingTable::new();
        let snap = table.snapshot();
        assert_eq!(snap.generation, 0);
        assert!(snap.channel_routes.is_empty());
    }

    #[test]
    fn batch_add_routes() {
        let table = RoutingTable::new();
        let c1 = cid();
        let c2 = cid();
        table.batch_update(vec![
            RoutingMutation::AddRoute {
                channel_id: 0x001,
                entry: entry(c1),
            },
            RoutingMutation::AddRoute {
                channel_id: 0x001,
                entry: entry(c2),
            },
            RoutingMutation::AddRoute {
                channel_id: 0x002,
                entry: entry(c1),
            },
        ]);
        let snap = table.snapshot();
        assert_eq!(snap.channel_routes[&0x001].len(), 2);
        assert_eq!(snap.channel_routes[&0x002].len(), 1);
        assert_eq!(snap.generation, 1);
    }

    #[test]
    fn remove_client_clears_all() {
        let table = RoutingTable::new();
        let c1 = cid();
        let c2 = cid();
        table.batch_update(vec![
            RoutingMutation::AddRoute {
                channel_id: 0x001,
                entry: entry(c1),
            },
            RoutingMutation::AddRoute {
                channel_id: 0x002,
                entry: entry(c1),
            },
            RoutingMutation::AddRoute {
                channel_id: 0x002,
                entry: entry(c2),
            },
        ]);
        table.batch_update(vec![RoutingMutation::RemoveClient(c1)]);
        let snap = table.snapshot();
        // Channel 0x001 should be gone (all entries removed)
        assert!(!snap.channel_routes.contains_key(&0x001));
        // Channel 0x002 should only have c2
        assert_eq!(snap.channel_routes[&0x002].len(), 1);
        assert_eq!(snap.channel_routes[&0x002][0].client_id, c2);
    }

    #[test]
    fn transfer_channel() {
        let table = RoutingTable::new();
        let c1 = cid();
        let c2 = cid();
        table.batch_update(vec![RoutingMutation::AddRoute {
            channel_id: 0x004,
            entry: entry(c1),
        }]);
        table.batch_update(vec![RoutingMutation::TransferChannel {
            channel_id: 0x004,
            from: c1,
            to_entry: entry(c2),
        }]);
        let snap = table.snapshot();
        assert_eq!(snap.channel_routes[&0x004].len(), 1);
        assert_eq!(snap.channel_routes[&0x004][0].client_id, c2);
    }

    #[test]
    fn generation_increments_per_batch() {
        let table = RoutingTable::new();
        let c = cid();
        table.batch_update(vec![RoutingMutation::AddRoute {
            channel_id: 0x001,
            entry: entry(c),
        }]);
        table.batch_update(vec![RoutingMutation::AddRoute {
            channel_id: 0x002,
            entry: entry(c),
        }]);
        table.batch_update(vec![RoutingMutation::AddRoute {
            channel_id: 0x003,
            entry: entry(c),
        }]);
        let snap = table.snapshot();
        assert_eq!(snap.generation, 3);
    }

    #[test]
    fn snapshot_consistency_old_preserved_after_mutation() {
        let table = RoutingTable::new();
        let c1 = cid();
        let c2 = cid();

        table.batch_update(vec![RoutingMutation::AddRoute {
            channel_id: 0x001,
            entry: entry(c1),
        }]);

        // Capture old snapshot before next mutation
        let old_snap = table.snapshot();

        table.batch_update(vec![RoutingMutation::AddRoute {
            channel_id: 0x001,
            entry: entry(c2),
        }]);

        // Old snapshot still shows only c1
        assert_eq!(old_snap.channel_routes[&0x001].len(), 1);
        assert_eq!(old_snap.generation, 1);

        // New snapshot shows both
        let new_snap = table.snapshot();
        assert_eq!(new_snap.channel_routes[&0x001].len(), 2);
        assert_eq!(new_snap.generation, 2);
    }
}
