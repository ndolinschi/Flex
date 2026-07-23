
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use keyring::Entry;
use rand::RngExt;

use crate::error::{DesktopError, DesktopResult};

const MASTER_KEY_SERVICE: &str = "Flex Safe Storage";
const MASTER_KEY_ACCOUNT: &str = "master";
const MASTER_KEY_LEN: usize = 32;

const SECRETS_FILE: &str = "secrets.enc";
const MASTER_KEY_FILE: &str = "master.key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretStorageMode {
    File,
    Keychain,
}

impl SecretStorageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SecretStorageMode::File => "file",
            SecretStorageMode::Keychain => "keychain",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "file" => Some(SecretStorageMode::File),
            "keychain" => Some(SecretStorageMode::Keychain),
            _ => None,
        }
    }
}

static CONFIGURED_MODE: OnceLock<Mutex<SecretStorageMode>> = OnceLock::new();

fn keychain_master_key_exists() -> bool {
    match Entry::new(MASTER_KEY_SERVICE, MASTER_KEY_ACCOUNT) {
        Ok(entry) => entry.get_password().is_ok(),
        Err(_) => false,
    }
}

pub fn resolve_mode(explicit: Option<&str>) -> SecretStorageMode {
    if !cfg!(target_os = "macos") {
        return SecretStorageMode::File;
    }
    if let Some(s) = explicit {
        if let Some(mode) = SecretStorageMode::parse(s) {
            return mode;
        }
    }
    if keychain_master_key_exists() {
        SecretStorageMode::Keychain
    } else {
        SecretStorageMode::File
    }
}

pub fn set_configured_mode(mode: SecretStorageMode) {
    match CONFIGURED_MODE.get() {
        Some(cell) => *cell.lock().expect("configured mode mutex poisoned") = mode,
        None => {
            let _ = CONFIGURED_MODE.set(Mutex::new(mode));
        }
    }
}

fn configured_mode() -> SecretStorageMode {
    CONFIGURED_MODE
        .get()
        .map(|cell| *cell.lock().expect("configured mode mutex poisoned"))
        .unwrap_or(SecretStorageMode::File)
}

fn master_key_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(MASTER_KEY_FILE)
}

static MASTER_KEY: OnceLock<Mutex<[u8; MASTER_KEY_LEN]>> = OnceLock::new();

type SecretsMap = BTreeMap<String, String>;

fn secrets_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SECRETS_FILE)
}

fn master_key(config_dir: &Path) -> DesktopResult<[u8; MASTER_KEY_LEN]> {
    if let Some(cached) = MASTER_KEY.get() {
        return Ok(*cached.lock().expect("master key mutex poisoned"));
    }

    let key_bytes = match configured_mode() {
        SecretStorageMode::File => load_or_create_file_master_key(config_dir)?,
        SecretStorageMode::Keychain => load_or_create_keychain_master_key()?,
    };

    let cached = MASTER_KEY.get_or_init(|| Mutex::new(key_bytes));
    Ok(*cached.lock().expect("master key mutex poisoned"))
}

fn load_or_create_keychain_master_key() -> DesktopResult<[u8; MASTER_KEY_LEN]> {
    let entry = Entry::new(MASTER_KEY_SERVICE, MASTER_KEY_ACCOUNT)
        .map_err(|e| DesktopError::Keychain(e.to_string()))?;

    match entry.get_password() {
        Ok(encoded) => decode_master_key(&encoded),
        Err(keyring::Error::NoEntry) => {
            let mut bytes = [0u8; MASTER_KEY_LEN];
            rand::rng().fill(&mut bytes);
            let encoded = base64_encode(&bytes);
            entry
                .set_password(&encoded)
                .map_err(|e| DesktopError::Keychain(e.to_string()))?;
            Ok(bytes)
        }
        Err(e) => Err(DesktopError::Keychain(e.to_string())),
    }
}

fn load_or_create_file_master_key(config_dir: &Path) -> DesktopResult<[u8; MASTER_KEY_LEN]> {
    let path = master_key_file_path(config_dir);
    match fs::read_to_string(&path) {
        Ok(encoded) => decode_master_key(&encoded),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut bytes = [0u8; MASTER_KEY_LEN];
            rand::rng().fill(&mut bytes);
            write_file_master_key(&path, &bytes)?;
            Ok(bytes)
        }
        Err(e) => Err(DesktopError::Config(format!(
            "failed to read master key file: {e}"
        ))),
    }
}

fn write_file_master_key(path: &Path, bytes: &[u8; MASTER_KEY_LEN]) -> DesktopResult<()> {
    let encoded = base64_encode(bytes);
    fs::write(path, encoded).map_err(|e| DesktopError::Config(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms).map_err(|e| DesktopError::Config(e.to_string()))?;
    }
    Ok(())
}

fn decode_master_key(encoded: &str) -> DesktopResult<[u8; MASTER_KEY_LEN]> {
    let bytes = base64_decode(encoded)
        .map_err(|e| DesktopError::Keychain(format!("corrupt master key: {e}")))?;
    bytes
        .try_into()
        .map_err(|_| DesktopError::Keychain("master key has unexpected length".into()))
}

fn load_secrets(config_dir: &Path) -> DesktopResult<SecretsMap> {
    let path = secrets_path(config_dir);
    let raw = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SecretsMap::new()),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "failed to read secrets file, treating as empty");
            return Ok(SecretsMap::new());
        }
    };

    let key = master_key(config_dir)?;
    match decrypt(&key, &raw) {
        Ok(plaintext) => match serde_json::from_slice(&plaintext) {
            Ok(map) => Ok(map),
            Err(e) => {
                tracing::warn!(error = %e, "secrets file JSON is corrupt, treating as empty");
                Ok(SecretsMap::new())
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "secrets file failed to decrypt, treating as empty");
            Ok(SecretsMap::new())
        }
    }
}

fn save_secrets(config_dir: &Path, secrets: &SecretsMap) -> DesktopResult<()> {
    let path = secrets_path(config_dir);
    if secrets.is_empty() {
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        return Ok(());
    }
    let key = master_key(config_dir)?;
    let plaintext = serde_json::to_vec(secrets).map_err(|e| DesktopError::Config(e.to_string()))?;
    let ciphertext = encrypt(&key, &plaintext)?;
    fs::write(&path, ciphertext).map_err(|e| DesktopError::Config(e.to_string()))
}

fn encrypt(key: &[u8; MASTER_KEY_LEN], plaintext: &[u8]) -> DesktopResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(*key));
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| DesktopError::Config(format!("encryption failed: {e}")))?;
    let mut out = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt(key: &[u8; MASTER_KEY_LEN], data: &[u8]) -> DesktopResult<Vec<u8>> {
    if data.len() < 12 {
        return Err(DesktopError::Config("secrets file too short".into()));
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(*key));
    let nonce_arr: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|_| DesktopError::Config("invalid nonce length".into()))?;
    let nonce = Nonce::from(nonce_arr);
    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| DesktopError::Config(format!("decryption failed: {e}")))
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("invalid base64 byte: {c}")),
        }
    }
    let s = s.trim();
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("invalid base64 length".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&b| b == b'=').count();
        let c0 = val(chunk[0])?;
        let c1 = val(chunk[1])?;
        let c2 = if chunk[2] == b'=' { 0 } else { val(chunk[2])? };
        let c3 = if chunk[3] == b'=' { 0 } else { val(chunk[3])? };
        let n = ((c0 as u32) << 18) | ((c1 as u32) << 12) | ((c2 as u32) << 6) | (c3 as u32);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

fn migrate_legacy_entry(secrets: &mut SecretsMap, service: &str, account: &str, key_id: &str) {
    if secrets.contains_key(key_id) {
        return;
    }
    let entry = match Entry::new(service, account) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, service, account, "migration: failed to open legacy keychain entry");
            return;
        }
    };
    match entry.get_password() {
        Ok(raw) => {
            secrets.insert(key_id.to_owned(), raw);
            if let Err(e) = entry.delete_credential() {
                tracing::warn!(error = %e, service, account, "migration: failed to delete legacy keychain entry");
            } else {
                tracing::info!(
                    service,
                    account,
                    "migrated legacy keychain entry to encrypted secrets store"
                );
            }
        }
        Err(keyring::Error::NoEntry) => {}
        Err(e) => {
            tracing::warn!(error = %e, service, account, "migration: failed to read legacy keychain entry");
        }
    }
}

pub struct SecretsStore;

impl SecretsStore {
    pub fn load_all(
        config_dir: &Path,
        legacy_migrations: &[(&str, &str, &str)],
    ) -> DesktopResult<SecretsMap> {
        let mut secrets = load_secrets(config_dir)?;
        let mut migrated = false;
        for (service, account, key_id) in legacy_migrations {
            let before = secrets.len();
            migrate_legacy_entry(&mut secrets, service, account, key_id);
            if secrets.len() != before || secrets.contains_key(*key_id) {
                migrated = true;
            }
        }
        if migrated {
            save_secrets(config_dir, &secrets)?;
        }
        Ok(secrets)
    }

    pub fn save_all(config_dir: &Path, secrets: &SecretsMap) -> DesktopResult<()> {
        save_secrets(config_dir, secrets)
    }

    pub fn switch_mode(
        config_dir: &Path,
        from: SecretStorageMode,
        to: SecretStorageMode,
    ) -> DesktopResult<()> {
        if from == to {
            return Ok(());
        }

        let key_bytes = match from {
            SecretStorageMode::File => load_or_create_file_master_key(config_dir)?,
            SecretStorageMode::Keychain => load_or_create_keychain_master_key()?,
        };

        match to {
            SecretStorageMode::File => {
                write_file_master_key(&master_key_file_path(config_dir), &key_bytes)?;
            }
            SecretStorageMode::Keychain => {
                let entry = Entry::new(MASTER_KEY_SERVICE, MASTER_KEY_ACCOUNT)
                    .map_err(|e| DesktopError::Keychain(e.to_string()))?;
                entry
                    .set_password(&base64_encode(&key_bytes))
                    .map_err(|e| DesktopError::Keychain(e.to_string()))?;
            }
        }

        if let Err(e) = load_secrets_with_key(config_dir, &key_bytes) {
            tracing::warn!(error = %e, "post-migration decrypt check failed; leaving old backend in place");
            return Err(e);
        }

        MASTER_KEY.get_or_init(|| Mutex::new(key_bytes));
        if let Some(cell) = MASTER_KEY.get() {
            *cell.lock().expect("master key mutex poisoned") = key_bytes;
        }
        set_configured_mode(to);

        match from {
            SecretStorageMode::File => {
                let path = master_key_file_path(config_dir);
                if path.exists() {
                    let _ = fs::remove_file(&path);
                }
            }
            SecretStorageMode::Keychain => {
                if let Ok(entry) = Entry::new(MASTER_KEY_SERVICE, MASTER_KEY_ACCOUNT) {
                    if let Err(e) = entry.delete_credential() {
                        tracing::warn!(error = %e, "failed to delete old keychain master key after switching to file mode");
                    }
                }
            }
        }

        Ok(())
    }
}

fn load_secrets_with_key(
    config_dir: &Path,
    key: &[u8; MASTER_KEY_LEN],
) -> DesktopResult<SecretsMap> {
    let path = secrets_path(config_dir);
    let raw = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SecretsMap::new()),
        Err(e) => {
            return Err(DesktopError::Config(format!(
                "failed to read secrets file during mode switch: {e}"
            )));
        }
    };
    let plaintext = decrypt(key, &raw)?;
    serde_json::from_slice(&plaintext).map_err(|e| DesktopError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        for len in [0, 1, 2, 3, 4, 5, 31, 32, 33] {
            let bytes: Vec<u8> = (0..len as u8).collect();
            let encoded = base64_encode(&bytes);
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(decoded, bytes, "roundtrip failed for len {len}");
        }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [7u8; MASTER_KEY_LEN];
        let plaintext = b"{\"default\":\"sk-test-123\"}".to_vec();
        let ciphertext = encrypt(&key, &plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_rejects_wrong_key() {
        let key = [7u8; MASTER_KEY_LEN];
        let other_key = [9u8; MASTER_KEY_LEN];
        let ciphertext = encrypt(&key, b"secret").unwrap();
        assert!(decrypt(&other_key, &ciphertext).is_err());
    }
}
