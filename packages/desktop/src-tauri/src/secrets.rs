//! Encrypted local secrets store, mirroring the reference design's
//! `safeStorage` pattern (used by many Electron/Tauri-adjacent apps).
//!
//! macOS Keychain prompts the user on every access to an item unless the
//! calling binary's code-signature is stable *and* the user picked
//! "Always Allow". Dev builds are ad-hoc signed and re-sign on every
//! rebuild, so the previous design — one keychain item per provider
//! profile (`PROFILE_KEYS_ACCOUNT`/`profile_keys` map, read/written on
//! basically every config load/save) — meant the user got asked for
//! Keychain access constantly.
//!
//! The fix has two layers:
//!
//! 1. Only touch the Keychain **once per process** (when running in
//!    [`SecretStorageMode::Keychain`]) rather than per-secret. A single
//!    keychain entry ("Flex Safe Storage" / "master") holds 32 random bytes
//!    generated on first use, read once and cached in memory for the
//!    lifetime of the process (`OnceLock`).
//! 2. **Storage mode** ([`SecretStorageMode`]): the master key can instead
//!    live in a local file (`master.key`, mode 0600) right next to
//!    `secrets.enc`, touching the Keychain *zero* times, ever. This is now
//!    the default for new installs — see [`resolve_mode`].
//!
//! Either way, all actual secrets (provider API keys) live in a small JSON
//! blob on local disk (`secrets.enc`, next to `provider_prefs.json`),
//! AES-256-GCM encrypted with the master key and a random nonce prepended
//! to each write. Only the master key's *location* differs between modes;
//! `secrets.enc` itself is identical either way, so switching modes is just
//! a matter of moving the 32 key bytes to the other backend (see
//! [`switch_mode`]).
//!
//! Net effect in Keychain mode: at most one Keychain prompt per app launch
//! (the first read/creation of the master key), and none at all once the
//! user grants "Always Allow" against a stable signing identity. In File
//! mode: no Keychain prompts at all, ever — the trade-off (documented to the
//! user in Settings) is that the master key sits in a file readable by the
//! local user account, rather than behind the OS-level Keychain protection.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use keyring::Entry;
use rand::RngExt;

use crate::error::{DesktopError, DesktopResult};

/// Keychain service for the single master-key item. Distinct from the old
/// per-profile-key service (`agentloop.desktop`) so the two coexist during
/// migration.
const MASTER_KEY_SERVICE: &str = "Flex Safe Storage";
const MASTER_KEY_ACCOUNT: &str = "master";
const MASTER_KEY_LEN: usize = 32; // AES-256

const SECRETS_FILE: &str = "secrets.enc";
/// File-mode master key file name, kept next to `secrets.enc`.
const MASTER_KEY_FILE: &str = "master.key";

/// Where the master key (and therefore all secret decryption) is anchored.
///
/// `File` is the default for brand-new installs: zero Keychain prompts,
/// ever. `Keychain` is the opt-in, OS-protected alternative — see the module
/// doc comment and [`resolve_mode`] for the existing-install compatibility
/// rule.
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

/// Process-wide configured mode, set once at `load_config` time (see
/// `config::load_config`) and read by every subsequent `SecretsStore` call.
/// Kept as a simple `OnceLock` (not a `Mutex`) because `load_config` runs
/// exactly once per process before any secrets are touched, and mode
/// *switches* go through [`switch_mode`], which updates this cell directly
/// rather than requiring a fresh process.
static CONFIGURED_MODE: OnceLock<Mutex<SecretStorageMode>> = OnceLock::new();

/// Whether a Keychain master-key item already exists (best-effort check —
/// failures are treated as "doesn't exist" so callers fall back safely).
fn keychain_master_key_exists() -> bool {
    match Entry::new(MASTER_KEY_SERVICE, MASTER_KEY_ACCOUNT) {
        Ok(entry) => entry.get_password().is_ok(),
        Err(_) => false,
    }
}

/// Resolve the effective storage mode given an explicit pref (`None` means
/// "no explicit choice made yet") and honoring existing installs:
///
/// - Explicit pref set -> use it verbatim (but see the non-macOS override
///   below: `Keychain` can never win off of macOS).
/// - No explicit pref, but a Keychain master key already exists (i.e. this
///   is an *existing* install from before this feature, or one that already
///   ran in Keychain mode) -> resolve to `Keychain`, so we don't silently
///   switch an existing user's storage backend out from under them (which
///   would otherwise manifest as a surprise "create a new file key, keychain
///   item now orphaned" migration on next launch).
/// - No explicit pref, no existing Keychain item -> `File`, the default for
///   brand-new setups.
///
/// Product decision: the OS-keychain storage *mode* is macOS-only for now
/// (the `keyring` crate itself is cross-platform — Windows Credential
/// Manager, Linux secret-service — but we haven't qualified/tested those
/// backends for this app yet). On non-macOS targets this always resolves to
/// `File`, regardless of explicit pref or a pre-existing Keychain item, so
/// there is no path to `Keychain` mode outside macOS. `set_secret_storage`
/// (the explicit user-facing switch) additionally returns a clear error if
/// asked for `Keychain` on non-macOS — see `config::set_secret_storage`.
pub fn resolve_mode(explicit: Option<&str>) -> SecretStorageMode {
    if !cfg!(target_os = "macos") {
        // Keychain mode is macOS-only for now — force File on every other platform.
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

/// Set the process-wide configured mode. Called once from `load_config`
/// (and again after a successful [`switch_mode`]) so all subsequent
/// `SecretsStore`/`master_key` calls agree on where the key lives without
/// needing the mode threaded through every call site.
pub fn set_configured_mode(mode: SecretStorageMode) {
    match CONFIGURED_MODE.get() {
        Some(cell) => *cell.lock().expect("configured mode mutex poisoned") = mode,
        None => {
            let _ = CONFIGURED_MODE.set(Mutex::new(mode));
        }
    }
}

/// The current process-wide configured mode, defaulting to `File` if never
/// explicitly set (shouldn't happen in practice — `load_config` always sets
/// it — but a safe fallback beats a panic).
fn configured_mode() -> SecretStorageMode {
    CONFIGURED_MODE
        .get()
        .map(|cell| *cell.lock().expect("configured mode mutex poisoned"))
        .unwrap_or(SecretStorageMode::File)
}

fn master_key_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(MASTER_KEY_FILE)
}

/// Master key, read from the Keychain (or created) at most once per process.
static MASTER_KEY: OnceLock<Mutex<[u8; MASTER_KEY_LEN]>> = OnceLock::new();

/// On-disk encrypted map: opaque key id -> secret (e.g. profile id -> API key).
type SecretsMap = BTreeMap<String, String>;

fn secrets_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SECRETS_FILE)
}

/// Load (creating if absent) the 32-byte master key, from whichever backend
/// [`configured_mode`] points at, caching it in memory for the rest of the
/// process. In Keychain mode this is the *only* Keychain touch in the app
/// after migration — everything else reads/writes the local encrypted file.
/// In File mode, no Keychain entry is ever created or read.
fn master_key(config_dir: &Path) -> DesktopResult<[u8; MASTER_KEY_LEN]> {
    if let Some(cached) = MASTER_KEY.get() {
        return Ok(*cached.lock().expect("master key mutex poisoned"));
    }

    let key_bytes = match configured_mode() {
        SecretStorageMode::File => load_or_create_file_master_key(config_dir)?,
        SecretStorageMode::Keychain => load_or_create_keychain_master_key()?,
    };

    // Another thread may have raced us to populate the OnceLock; either way
    // the winning value is what everyone reads from here on.
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

/// Load (creating if absent) the master key from the local `master.key`
/// file, base64-encoded, permissions restricted to the owner (0600) so
/// other local accounts can't read it. Zero Keychain touches.
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

/// Write the master key file and restrict its permissions to owner
/// read/write only (0600) on Unix (macOS/Linux). Windows has no POSIX mode
/// bits; `std::os::unix::fs::PermissionsExt` doesn't exist there, so this is
/// `#[cfg(unix)]`-gated and Windows instead relies on the default ACLs of the
/// per-user config directory (`%APPDATA%`), which already restrict access to
/// the owning user account — documented to the user in Settings alongside
/// the storage-mode choice.
fn write_file_master_key(path: &Path, bytes: &[u8; MASTER_KEY_LEN]) -> DesktopResult<()> {
    let encoded = base64_encode(bytes);
    fs::write(path, encoded).map_err(|e| DesktopError::Config(e.to_string()))?;
    // unix-only: POSIX permission bits don't exist on Windows; Windows relies on default ACLs.
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

/// Load the encrypted secrets map from disk, decrypting with the (cached)
/// master key. A missing file is an empty map. A corrupted/unreadable file
/// is logged and treated as empty rather than crashing the app — losing
/// stored keys is recoverable (re-enter them); crashing on launch is not.
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
        // Nothing left to protect — remove the file rather than persisting
        // an encrypted empty map.
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

// --- Minimal base64 (standard alphabet, padded) — avoids pulling in the
// `base64` crate for one small, internal use (encoding 32 raw bytes for
// keychain storage).
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

/// Best-effort migration of one legacy per-id Keychain entry into the
/// encrypted store. Reads the old entry (service/account given by the
/// caller), and if present, folds it into `secrets` (without overwriting an
/// existing encrypted-store value for the same id) and deletes the old
/// Keychain item so it stops prompting. Never fatal: any failure is logged
/// and treated as "nothing to migrate".
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

/// Public API mirroring the old per-profile keychain get/set/delete, so
/// callers in `config.rs` swap mechanically.
pub struct SecretsStore;

impl SecretsStore {
    /// Load every stored secret (e.g. profile id -> API key). If a
    /// `legacy_migrations` entry has no counterpart in the encrypted store
    /// yet, the corresponding old keychain item is migrated in (read, moved
    /// into the returned map, then deleted from the keychain) — see module
    /// docs. `legacy_migrations` is `(service, account, key_id)` triples.
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

    /// Persist the full secrets map, replacing whatever was on disk.
    pub fn save_all(config_dir: &Path, secrets: &SecretsMap) -> DesktopResult<()> {
        save_secrets(config_dir, secrets)
    }

    /// Move the master key from `from` to `to`, verifying `secrets.enc`
    /// still decrypts under the (unchanged) key material before deleting
    /// the old backend's copy. `secrets.enc` itself is never rewritten —
    /// only the key's location changes. A no-op (but still verifies
    /// decryption) if `from == to`.
    ///
    /// Steps: (1) read the current key from `from`, (2) write it to `to`,
    /// (3) re-load `secrets.enc` under the cached in-memory key to confirm
    /// nothing broke, (4) only then delete the old backend's copy. If step
    /// 2 or 3 fails, the old copy is left untouched so the user's secrets
    /// stay reachable under the previous mode.
    pub fn switch_mode(
        config_dir: &Path,
        from: SecretStorageMode,
        to: SecretStorageMode,
    ) -> DesktopResult<()> {
        if from == to {
            return Ok(());
        }

        // Read the key from the *current* backend directly (not through the
        // process-wide cache, which may already reflect a different mode by
        // the time this runs).
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

        // Verify secrets.enc (if any) still decrypts under the same key
        // material before touching the old backend. This never re-encrypts
        // anything — same key, same ciphertext — it's purely a sanity check
        // that the new backend actually holds a working copy.
        if let Err(e) = load_secrets_with_key(config_dir, &key_bytes) {
            tracing::warn!(error = %e, "post-migration decrypt check failed; leaving old backend in place");
            return Err(e);
        }

        // Update the process-wide cache + configured mode so subsequent
        // calls in this process use the new backend without a restart.
        MASTER_KEY.get_or_init(|| Mutex::new(key_bytes));
        if let Some(cell) = MASTER_KEY.get() {
            *cell.lock().expect("master key mutex poisoned") = key_bytes;
        }
        set_configured_mode(to);

        // Only now remove the old backend's copy.
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

/// Like `load_secrets`, but decrypts with an explicit key rather than going
/// through `master_key`/the process cache — used by `switch_mode` to verify
/// the new backend's key before deleting the old one.
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
