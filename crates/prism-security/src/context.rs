use std::collections::HashMap;
use std::sync::Arc;

use prism_protocol::channel::*;
use prism_protocol::header::PrismHeader;

use crate::filter::ContentFilter;
use crate::pairing::{PairingEntry, Permission};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDecision {
    AllowAll,
    Blocked,
    NeedsConfirmation,
    CheckFilter,
}

pub struct SecurityContext {
    pub device: Arc<PairingEntry>,
    pub channel_decisions: [ChannelDecision; 256],
    pub active_filters: HashMap<u16, Arc<dyn ContentFilter>>,
    pub is_0rtt_safe: [bool; 256],
}

impl SecurityContext {
    pub fn for_device(entry: Arc<PairingEntry>) -> Self {
        let mut channel_decisions = [ChannelDecision::AllowAll; 256];
        let mut is_0rtt_safe = [false; 256];

        let perms = &entry.permissions;
        Self::apply_permission(&mut channel_decisions, CHANNEL_DISPLAY, perms.display);
        Self::apply_permission(&mut channel_decisions, CHANNEL_INPUT, perms.input);
        Self::apply_permission(&mut channel_decisions, CHANNEL_CLIPBOARD, perms.clipboard);
        Self::apply_permission(&mut channel_decisions, CHANNEL_FILESHARE, perms.fileshare);
        Self::apply_permission(&mut channel_decisions, CHANNEL_NOTIFY, perms.notify);
        Self::apply_permission(&mut channel_decisions, CHANNEL_CAMERA, perms.camera);
        Self::apply_permission(&mut channel_decisions, CHANNEL_SENSOR, perms.sensor);

        is_0rtt_safe[(CHANNEL_DISPLAY & 0xFF) as usize] = true;
        is_0rtt_safe[(CHANNEL_INPUT & 0xFF) as usize] = true;
        is_0rtt_safe[(CHANNEL_AUDIO & 0xFF) as usize] = true;

        Self {
            device: entry,
            channel_decisions,
            active_filters: HashMap::new(),
            is_0rtt_safe,
        }
    }

    fn apply_permission(decisions: &mut [ChannelDecision; 256], channel_id: u16, permission: Permission) {
        decisions[(channel_id & 0xFF) as usize] = match permission {
            Permission::Allow => ChannelDecision::AllowAll,
            Permission::Deny => ChannelDecision::Blocked,
            Permission::Ask => ChannelDecision::NeedsConfirmation,
        };
    }

    #[inline(always)]
    pub fn channel_decision(&self, channel_id: u16) -> ChannelDecision {
        self.channel_decisions[(channel_id & 0xFF) as usize]
    }

    pub fn content_filter(&self, channel_id: u16) -> Option<&Arc<dyn ContentFilter>> {
        self.active_filters.get(&channel_id)
    }

    #[inline(always)]
    pub fn is_0rtt_safe(&self, header: &PrismHeader) -> bool {
        self.is_0rtt_safe[(header.channel_id & 0xFF) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;
    use crate::pairing::*;

    fn make_context(perms: ChannelPermissions) -> SecurityContext {
        let id = LocalIdentity::generate("Test");
        let entry = Arc::new(PairingEntry {
            device: id.identity,
            state: PairingState::Paired,
            permissions: perms,
            paired_at: 0,
            last_seen: 0,
        });
        SecurityContext::for_device(entry)
    }

    #[test]
    fn default_allows_core_channels() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_decision(CHANNEL_DISPLAY), ChannelDecision::AllowAll);
        assert_eq!(ctx.channel_decision(CHANNEL_INPUT), ChannelDecision::AllowAll);
        assert_eq!(ctx.channel_decision(CHANNEL_CLIPBOARD), ChannelDecision::AllowAll);
    }

    #[test]
    fn ask_maps_to_needs_confirmation() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_decision(CHANNEL_CAMERA), ChannelDecision::NeedsConfirmation);
        assert_eq!(ctx.channel_decision(CHANNEL_SENSOR), ChannelDecision::NeedsConfirmation);
    }

    #[test]
    fn deny_maps_to_blocked() {
        let mut perms = ChannelPermissions::default();
        perms.display = Permission::Deny;
        let ctx = make_context(perms);
        assert_eq!(ctx.channel_decision(CHANNEL_DISPLAY), ChannelDecision::Blocked);
    }

    #[test]
    fn zero_rtt_safe_channels() {
        let ctx = make_context(ChannelPermissions::default());
        let h = |ch| PrismHeader {
            version: 0, channel_id: ch, msg_type: 0,
            flags: 0, sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        assert!(ctx.is_0rtt_safe(&h(CHANNEL_DISPLAY)));
        assert!(ctx.is_0rtt_safe(&h(CHANNEL_INPUT)));
        assert!(ctx.is_0rtt_safe(&h(CHANNEL_AUDIO)));
        assert!(!ctx.is_0rtt_safe(&h(CHANNEL_CLIPBOARD)));
        assert!(!ctx.is_0rtt_safe(&h(CHANNEL_FILESHARE)));
    }

    #[test]
    fn no_active_filters_in_phase_1() {
        let ctx = make_context(ChannelPermissions::default());
        assert!(ctx.active_filters.is_empty());
        assert!(ctx.content_filter(CHANNEL_CLIPBOARD).is_none());
    }

    #[test]
    fn unknown_channel_defaults_to_allow() {
        let ctx = make_context(ChannelPermissions::default());
        assert_eq!(ctx.channel_decision(0x100), ChannelDecision::AllowAll);
    }
}
