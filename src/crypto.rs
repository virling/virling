use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::Context;
use sha2::{Digest, Sha256};

/// HMAC-SHA256 of `msg` with `key`.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner = Sha256::new()
        .chain_update(block.map(|b| b ^ 0x36))
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(block.map(|b| b ^ 0x5c))
        .chain_update(inner)
        .finalize()
        .into()
}

/// Encrypts payment-provider keys at rest with AES-256-GCM.
pub struct Vault {
    cipher: Aes256Gcm,
}

impl Vault {
    /// Uses VIRLING_SECRET_KEY (64 hex chars) when set, else a key file created once in the data dir.
    pub fn load(data_dir: &Path, env_key: Option<&str>) -> anyhow::Result<Self> {
        let bytes = match env_key {
            Some(hex) => {
                decode_hex(hex.trim()).context("VIRLING_SECRET_KEY must be 64 hex characters")?
            }
            None => {
                let path = data_dir.join("secret.key");
                if path.exists() {
                    std::fs::read(&path)?
                } else {
                    let mut key = vec![0u8; 32];
                    getrandom::fill(&mut key).expect("system randomness");
                    write_private(&path, &key)?;
                    key
                }
            }
        };
        anyhow::ensure!(bytes.len() == 32, "secret key must be 32 bytes");
        Ok(Self {
            cipher: Aes256Gcm::new(&Key::<Aes256Gcm>::try_from(bytes.as_slice())?),
        })
    }

    pub fn encrypt(&self, plaintext: &str) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).expect("system randomness");
        let ciphertext = self
            .cipher
            .encrypt(&Nonce::from(nonce), plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        Ok((ciphertext, nonce.to_vec()))
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> anyhow::Result<String> {
        let plain = self
            .cipher
            .decrypt(&Nonce::try_from(nonce)?, ciphertext)
            .map_err(|_| anyhow::anyhow!("decryption failed, wrong secret key?"))?;
        Ok(String::from_utf8(plain)?)
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    s.len()
        .is_multiple_of(2)
        .then(|| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
                .collect()
        })
        .flatten()
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    Ok(std::fs::write(path, bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_persists_key() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::load(dir.path(), None).unwrap();
        let (c, n) = vault.encrypt("rk_live_123").unwrap();
        assert_ne!(c, b"rk_live_123");
        let again = Vault::load(dir.path(), None).unwrap();
        assert_eq!(again.decrypt(&c, &n).unwrap(), "rk_live_123");
        let other = Vault::load(dir.path(), Some(&"ab".repeat(32))).unwrap();
        assert!(other.decrypt(&c, &n).is_err());
    }
}
