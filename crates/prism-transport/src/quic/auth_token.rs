// QUIC address validation token using HMAC.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub fn generate_throughput_token(
    session_secret: &[u8; 32],
    device_id: &Uuid,
    expires_at: u64,
) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(session_secret).unwrap();
    mac.update(b"throughput");
    mac.update(device_id.as_bytes());
    mac.update(&expires_at.to_le_bytes());
    let result = mac.finalize();
    let mut token = [0u8; 32];
    token.copy_from_slice(&result.into_bytes());
    token
}

pub fn validate_throughput_token(
    session_secret: &[u8; 32],
    device_id: &Uuid,
    expires_at: u64,
    token: &[u8; 32],
    current_time: u64,
) -> bool {
    if current_time > expires_at {
        return false;
    }
    let mut mac = HmacSha256::new_from_slice(session_secret).unwrap();
    mac.update(b"throughput");
    mac.update(device_id.as_bytes());
    mac.update(&expires_at.to_le_bytes());
    mac.verify_slice(token).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip() {
        let secret = [42u8; 32];
        let device_id = Uuid::from_bytes([1; 16]);
        let token = generate_throughput_token(&secret, &device_id, 1_000_000);
        assert!(validate_throughput_token(&secret, &device_id, 1_000_000, &token, 500_000));
    }

    #[test]
    fn token_wrong_secret_fails() {
        let token = generate_throughput_token(&[42u8; 32], &Uuid::from_bytes([1; 16]), 1_000_000);
        assert!(!validate_throughput_token(
            &[99u8; 32],
            &Uuid::from_bytes([1; 16]),
            1_000_000,
            &token,
            0
        ));
    }

    #[test]
    fn token_wrong_device_fails() {
        let token = generate_throughput_token(&[42u8; 32], &Uuid::from_bytes([1; 16]), 1_000_000);
        assert!(!validate_throughput_token(
            &[42u8; 32],
            &Uuid::from_bytes([2; 16]),
            1_000_000,
            &token,
            0
        ));
    }

    #[test]
    fn token_expired_fails() {
        let token = generate_throughput_token(&[42u8; 32], &Uuid::from_bytes([1; 16]), 1_000);
        assert!(!validate_throughput_token(
            &[42u8; 32],
            &Uuid::from_bytes([1; 16]),
            1_000,
            &token,
            2_000
        ));
    }

    #[test]
    fn different_tokens_for_different_devices() {
        let tok1 = generate_throughput_token(&[42u8; 32], &Uuid::from_bytes([1; 16]), 1_000_000);
        let tok2 = generate_throughput_token(&[42u8; 32], &Uuid::from_bytes([2; 16]), 1_000_000);
        assert_ne!(tok1, tok2);
    }
}
