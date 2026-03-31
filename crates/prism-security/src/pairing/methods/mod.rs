pub mod manual;
pub mod spake2;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PairingMethod {
    Manual,
    Tailscale,
    ShortCode,
    QrCode,
    Coordination,
}
