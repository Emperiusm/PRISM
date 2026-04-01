use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
}

pub fn hkdf_derive(secret: &[u8; 32], context: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, secret);
    let mut output = [0u8; 32];
    hk.expand(context.as_bytes(), &mut output).expect("HKDF expand failed");
    output
}

pub fn encrypt_aes_gcm(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|_| CryptoError::EncryptionFailed)?;
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

pub fn decrypt_aes_gcm(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if data.len() < 12 { return Err(CryptoError::DecryptionFailed); }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ciphertext).map_err(|_| CryptoError::DecryptionFailed)
}

pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() { return 0.0; }
    let mut counts = [0u32; 256];
    for &byte in data { counts[byte as usize] += 1; }
    let len = data.len() as f64;
    counts.iter().filter(|&&c| c > 0).map(|&c| { let p = c as f64 / len; -p * p.log2() }).sum()
}

pub fn is_high_entropy(data: &[u8]) -> bool {
    let len = data.len();
    if !(8..=128).contains(&len) { return false; }
    shannon_entropy(data) > 4.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hkdf_deterministic() {
        let secret = [42u8; 32];
        assert_eq!(hkdf_derive(&secret, "test"), hkdf_derive(&secret, "test"));
    }

    #[test]
    fn hkdf_different_contexts() {
        let secret = [42u8; 32];
        assert_ne!(hkdf_derive(&secret, "a"), hkdf_derive(&secret, "b"));
    }

    #[test]
    fn aes_gcm_roundtrip() {
        let key = [1u8; 32];
        let encrypted = encrypt_aes_gcm(&key, b"hello PRISM").unwrap();
        let decrypted = decrypt_aes_gcm(&key, &encrypted).unwrap();
        assert_eq!(decrypted, b"hello PRISM");
    }

    #[test]
    fn aes_gcm_wrong_key_fails() {
        let encrypted = encrypt_aes_gcm(&[1u8; 32], b"secret").unwrap();
        assert!(decrypt_aes_gcm(&[2u8; 32], &encrypted).is_err());
    }

    #[test]
    fn aes_gcm_tampered_fails() {
        let key = [1u8; 32];
        let mut encrypted = encrypt_aes_gcm(&key, b"secret").unwrap();
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0x01;
        assert!(decrypt_aes_gcm(&key, &encrypted).is_err());
    }

    #[test]
    fn aes_gcm_too_short_fails() {
        assert!(decrypt_aes_gcm(&[1u8; 32], &[0u8; 5]).is_err());
    }

    #[test]
    fn entropy_empty() { assert_eq!(shannon_entropy(&[]), 0.0); }

    #[test]
    fn entropy_uniform_low() { assert!(shannon_entropy(&vec![b'a'; 100]) < 0.01); }

    #[test]
    fn entropy_random_high() {
        let data: Vec<u8> = (0..=255).cycle().take(1024).collect();
        assert!(shannon_entropy(&data) > 7.9);
    }

    #[test]
    fn is_high_entropy_detects_secrets() {
        let key: Vec<u8> = (0..32).map(|i| (i * 7 + 13) as u8).collect();
        assert!(is_high_entropy(&key));
    }

    #[test]
    fn is_high_entropy_rejects_normal_text() {
        assert!(!is_high_entropy(b"hello world"));
    }
}
