// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use std::collections::{HashMap, HashSet};

use prism_protocol::channel::{
    CHANNEL_AUDIO, CHANNEL_CAMERA, CHANNEL_CLIPBOARD, CHANNEL_CONTROL, CHANNEL_DEVICE,
    CHANNEL_DISPLAY, CHANNEL_FILESHARE, CHANNEL_INPUT, CHANNEL_NOTIFY, CHANNEL_SENSOR,
    CHANNEL_TOUCH, EXTENSION_CHANNEL_START,
};

use crate::types::ClientId;
use crate::error::SessionError;

/// Policy governing how a transferable channel changes hands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferPolicy {
    /// Any client can request the channel; transfer is automatic.
    OnRequest,
    /// The current owner must approve the transfer.
    OwnerApproves,
    /// The server decides who gets the channel.
    ServerDecides,
}

/// Who owns or subscribes to a channel.
#[derive(Debug, Clone)]
pub enum ChannelOwnership {
    /// Only one client may hold this channel at a time.
    Exclusive { owner: Option<ClientId> },
    /// Any number of clients may subscribe.
    Shared { subscribers: HashSet<ClientId> },
    /// One client holds it but ownership can be transferred per policy.
    Transferable {
        owner: Option<ClientId>,
        transfer_policy: TransferPolicy,
    },
}

/// Result returned by `request_channel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelGrantResult {
    Granted,
    AlreadyOwned,
    Denied { reason: String, current_owner: Option<ClientId> },
    Transferred { from: ClientId },
    PendingApproval { current_owner: ClientId },
}

/// Registry of all channels and their ownership state.
pub struct ChannelRegistry {
    channels: HashMap<u16, ChannelOwnership>,
}

impl ChannelRegistry {
    /// Create a registry populated with the default PRISM channel set.
    pub fn with_defaults() -> Self {
        let mut channels = HashMap::new();

        // Exclusive channels
        for id in [CHANNEL_DISPLAY, CHANNEL_INPUT, CHANNEL_CAMERA] {
            channels.insert(id, ChannelOwnership::Exclusive { owner: None });
        }

        // Shared channels
        for id in [
            CHANNEL_CLIPBOARD,
            CHANNEL_CONTROL,
            CHANNEL_FILESHARE,
            CHANNEL_AUDIO,
            CHANNEL_DEVICE,
            CHANNEL_NOTIFY,
            CHANNEL_SENSOR,
        ] {
            channels.insert(id, ChannelOwnership::Shared { subscribers: HashSet::new() });
        }

        // Transferable channels
        channels.insert(
            CHANNEL_TOUCH,
            ChannelOwnership::Transferable {
                owner: None,
                transfer_policy: TransferPolicy::OnRequest,
            },
        );

        Self { channels }
    }

    /// Request ownership / subscription of a channel for a client.
    ///
    /// Extension channels (>= EXTENSION_CHANNEL_START) are auto-created as Shared.
    pub fn request_channel(
        &mut self,
        channel_id: u16,
        client_id: ClientId,
    ) -> Result<ChannelGrantResult, SessionError> {
        // Auto-create extension channels as Shared
        if channel_id >= EXTENSION_CHANNEL_START && !self.channels.contains_key(&channel_id) {
            self.channels.insert(
                channel_id,
                ChannelOwnership::Shared { subscribers: HashSet::new() },
            );
        }

        let ownership = self
            .channels
            .get_mut(&channel_id)
            .ok_or(SessionError::UnknownChannel(channel_id))?;

        match ownership {
            ChannelOwnership::Exclusive { owner } => match owner {
                None => {
                    *owner = Some(client_id);
                    Ok(ChannelGrantResult::Granted)
                }
                Some(current) if *current == client_id => Ok(ChannelGrantResult::AlreadyOwned),
                Some(current) => Ok(ChannelGrantResult::Denied {
                    reason: "channel exclusively held".to_string(),
                    current_owner: Some(*current),
                }),
            },
            ChannelOwnership::Shared { subscribers } => {
                if subscribers.contains(&client_id) {
                    Ok(ChannelGrantResult::AlreadyOwned)
                } else {
                    subscribers.insert(client_id);
                    Ok(ChannelGrantResult::Granted)
                }
            }
            ChannelOwnership::Transferable { owner, transfer_policy } => match owner {
                None => {
                    *owner = Some(client_id);
                    Ok(ChannelGrantResult::Granted)
                }
                Some(current) if *current == client_id => Ok(ChannelGrantResult::AlreadyOwned),
                Some(current) => match transfer_policy {
                    TransferPolicy::OnRequest => {
                        let from = *current;
                        *owner = Some(client_id);
                        Ok(ChannelGrantResult::Transferred { from })
                    }
                    TransferPolicy::OwnerApproves => {
                        Ok(ChannelGrantResult::PendingApproval { current_owner: *current })
                    }
                    TransferPolicy::ServerDecides => Ok(ChannelGrantResult::Denied {
                        reason: "server decides allocation".to_string(),
                        current_owner: Some(*current),
                    }),
                },
            },
        }
    }

    /// Release all channel holdings for a given client.
    pub fn release_all(&mut self, client_id: ClientId) {
        for ownership in self.channels.values_mut() {
            match ownership {
                ChannelOwnership::Exclusive { owner } => {
                    if *owner == Some(client_id) {
                        *owner = None;
                    }
                }
                ChannelOwnership::Shared { subscribers } => {
                    subscribers.remove(&client_id);
                }
                ChannelOwnership::Transferable { owner, .. } => {
                    if *owner == Some(client_id) {
                        *owner = None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn id() -> ClientId {
        Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext))
    }

    #[test]
    fn exclusive_grant_first_client() {
        let mut reg = ChannelRegistry::with_defaults();
        let c = id();
        let result = reg.request_channel(CHANNEL_DISPLAY, c).unwrap();
        assert_eq!(result, ChannelGrantResult::Granted);
    }

    #[test]
    fn exclusive_already_owned_same_client() {
        let mut reg = ChannelRegistry::with_defaults();
        let c = id();
        reg.request_channel(CHANNEL_DISPLAY, c).unwrap();
        let result = reg.request_channel(CHANNEL_DISPLAY, c).unwrap();
        assert_eq!(result, ChannelGrantResult::AlreadyOwned);
    }

    #[test]
    fn exclusive_denied_different_client() {
        let mut reg = ChannelRegistry::with_defaults();
        let c1 = id();
        let c2 = id();
        reg.request_channel(CHANNEL_DISPLAY, c1).unwrap();
        let result = reg.request_channel(CHANNEL_DISPLAY, c2).unwrap();
        match result {
            ChannelGrantResult::Denied { current_owner, .. } => {
                assert_eq!(current_owner, Some(c1));
            }
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[test]
    fn shared_multi_client() {
        let mut reg = ChannelRegistry::with_defaults();
        let c1 = id();
        let c2 = id();
        assert_eq!(reg.request_channel(CHANNEL_CLIPBOARD, c1).unwrap(), ChannelGrantResult::Granted);
        assert_eq!(reg.request_channel(CHANNEL_CLIPBOARD, c2).unwrap(), ChannelGrantResult::Granted);
        // Subscribing again is AlreadyOwned
        assert_eq!(reg.request_channel(CHANNEL_CLIPBOARD, c1).unwrap(), ChannelGrantResult::AlreadyOwned);
    }

    #[test]
    fn transferable_on_request() {
        let mut reg = ChannelRegistry::with_defaults();
        let c1 = id();
        let c2 = id();
        reg.request_channel(CHANNEL_TOUCH, c1).unwrap();
        let result = reg.request_channel(CHANNEL_TOUCH, c2).unwrap();
        assert_eq!(result, ChannelGrantResult::Transferred { from: c1 });
    }

    #[test]
    fn release_all_clears_exclusive() {
        let mut reg = ChannelRegistry::with_defaults();
        let c = id();
        reg.request_channel(CHANNEL_DISPLAY, c).unwrap();
        reg.request_channel(CHANNEL_INPUT, c).unwrap();
        reg.release_all(c);
        // After release, another client can claim
        let c2 = id();
        assert_eq!(reg.request_channel(CHANNEL_DISPLAY, c2).unwrap(), ChannelGrantResult::Granted);
        assert_eq!(reg.request_channel(CHANNEL_INPUT, c2).unwrap(), ChannelGrantResult::Granted);
    }

    #[test]
    fn release_all_clears_shared() {
        let mut reg = ChannelRegistry::with_defaults();
        let c1 = id();
        let c2 = id();
        reg.request_channel(CHANNEL_CLIPBOARD, c1).unwrap();
        reg.request_channel(CHANNEL_CLIPBOARD, c2).unwrap();
        reg.release_all(c1);
        // c1 removed; c2 can still re-subscribe (already subscribed, not changed by release_all on c1)
        assert_eq!(reg.request_channel(CHANNEL_CLIPBOARD, c1).unwrap(), ChannelGrantResult::Granted);
    }

    #[test]
    fn unknown_channel_error() {
        let mut reg = ChannelRegistry::with_defaults();
        let c = id();
        // 0x050 is not a defined channel and not an extension channel
        let err = reg.request_channel(0x050, c).unwrap_err();
        match err {
            SessionError::UnknownChannel(id) => assert_eq!(id, 0x050),
            other => panic!("expected UnknownChannel, got {other:?}"),
        }
    }

    #[test]
    fn extension_auto_shared() {
        let mut reg = ChannelRegistry::with_defaults();
        let c1 = id();
        let c2 = id();
        // EXTENSION_CHANNEL_START is 0x100
        assert_eq!(
            reg.request_channel(EXTENSION_CHANNEL_START, c1).unwrap(),
            ChannelGrantResult::Granted
        );
        assert_eq!(
            reg.request_channel(EXTENSION_CHANNEL_START, c2).unwrap(),
            ChannelGrantResult::Granted
        );
    }
}
