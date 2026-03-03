use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::Rng;

pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    pub fn new(master_key: &str) -> Self {
        // Derive a 32-byte key from master_key
        let key_bytes = derive_key(master_key);
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Crypto { cipher }
    }

    pub fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
        let mut rng = rand::thread_rng();
        let nonce_bytes: [u8; 12] = rng.gen();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption error: {}", e))?;

        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ciphertext);
        Ok(STANDARD.encode(&combined))
    }

    pub fn decrypt(&self, encoded: &str) -> anyhow::Result<String> {
        let combined = STANDARD
            .decode(encoded)
            .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))?;

        if combined.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data"));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption error: {}", e))?;

        String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("UTF-8 error: {}", e))
    }
}

fn derive_key(master_key: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    let bytes = master_key.as_bytes();
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = bytes[i % bytes.len()];
    }
    // XOR with rotated version for slightly better distribution
    let mut rotated = [0u8; 32];
    for (i, byte) in rotated.iter_mut().enumerate() {
        *byte = bytes[(i + 7) % bytes.len()];
    }
    for i in 0..32 {
        key[i] ^= rotated[i];
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let crypto = Crypto::new("test-master-key");
        let plaintext = "hello world secret";
        let encrypted = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }
}
