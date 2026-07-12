//! `PendingMap`: park an async caller on a keyed one-shot until someone
//! resolves it — the mechanic behind permission requests and user questions,
//! shared by the native loop and delegator adapters so both behave
//! identically.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::oneshot;

/// Keyed pending request registry.
pub struct PendingMap<K, V> {
    inner: Mutex<HashMap<K, oneshot::Sender<V>>>,
}

impl<K: Eq + Hash + Clone, V> Default for PendingMap<K, V> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone, V> PendingMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `key` and wait for its resolution. Returns `None` on timeout
    /// (the entry is cleaned up) or if the map is torn down.
    pub async fn wait(&self, key: K, timeout: Duration) -> Option<V> {
        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            map.insert(key.clone(), tx);
        }
        let result = tokio::time::timeout(timeout, rx).await;
        match result {
            Ok(Ok(value)) => Some(value),
            _ => {
                let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
                map.remove(&key);
                None
            }
        }
    }

    /// Resolve a pending key. Returns `false` if nothing was waiting.
    pub fn resolve(&self, key: &K, value: V) -> bool {
        let sender = {
            let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            map.remove(key)
        };
        match sender {
            Some(tx) => tx.send(value).is_ok(),
            None => false,
        }
    }

    /// Drop all pending waiters (their `wait` calls return `None`).
    pub fn clear(&self) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.clear();
    }

    pub fn is_pending(&self, key: &K) -> bool {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.contains_key(key)
    }
}
