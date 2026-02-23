use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;

use crate::error::{Result, VstorageError};

/// Derive a 256-bit key from password + salt using Argon2id.
pub fn derive_key(password: &str, salt: &[u8; 16]) -> [u8; 32] {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("Argon2 key derivation failed");
    key
}

/// Encrypt data with AES-256-GCM.
/// Returns (ciphertext_with_tag, nonce, salt).
pub fn encrypt(data: &[u8], password: &str) -> Result<(Vec<u8>, [u8; 12], [u8; 16])> {
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    rand::fill(&mut salt);
    rand::fill(&mut nonce_bytes);

    let key = derive_key(password, &salt);
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| VstorageError::Crypto(e.to_string()))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| VstorageError::Crypto(e.to_string()))?;

    Ok((ciphertext, nonce_bytes, salt))
}

/// Decrypt data with AES-256-GCM.
pub fn decrypt(
    ciphertext: &[u8],
    password: &str,
    nonce_bytes: &[u8; 12],
    salt: &[u8; 16],
) -> Result<Vec<u8>> {
    let key = derive_key(password, salt);
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| VstorageError::Crypto(e.to_string()))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| VstorageError::Crypto(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = b"Secret data for Vstorage testing!";
        let password = "hunter2";

        let (ciphertext, nonce, salt) = encrypt(plaintext, password).unwrap();
        assert_ne!(&ciphertext[..], &plaintext[..]);

        let decrypted = decrypt(&ciphertext, password, &nonce, &salt).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_wrong_password_fails() {
        let plaintext = b"Secret data";
        let (ciphertext, nonce, salt) = encrypt(plaintext, "correct").unwrap();
        assert!(decrypt(&ciphertext, "wrong", &nonce, &salt).is_err());
    }

    #[test]
    fn test_key_derivation_deterministic() {
        let salt = [42u8; 16];
        let k1 = derive_key("password", &salt);
        let k2 = derive_key("password", &salt);
        assert_eq!(k1, k2);
    }
}
