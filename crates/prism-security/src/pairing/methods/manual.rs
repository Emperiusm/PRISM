// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

use crate::identity::DeviceIdentity;
use crate::pairing::{ChannelPermissions, PairingEntry, PairingState};

pub fn pair_manually(remote: DeviceIdentity) -> PairingEntry {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    PairingEntry {
        device: remote,
        state: PairingState::Paired,
        permissions: ChannelPermissions::default(),
        paired_at: now,
        last_seen: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LocalIdentity;

    #[test]
    fn manual_pairing_creates_entry() {
        let remote = LocalIdentity::generate("Remote");
        let entry = pair_manually(remote.identity);
        assert_eq!(entry.state, PairingState::Paired);
        assert_eq!(entry.device.display_name, "Remote");
    }
}
