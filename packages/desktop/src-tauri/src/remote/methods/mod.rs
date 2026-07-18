//! Connection-method adapters — ingress onto the shared desktop Remote API.

mod bluetooth;
mod bonjour;
mod cloudflare;
mod tcp;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::config::RemoteAccessConfig;
use super::pairing::PairingEndpoint;
use crate::error::DesktopResult;

pub use bluetooth::BluetoothMethod;
pub use bonjour::BonjourMethod;
pub use cloudflare::CloudflareMethod;
pub use tcp::TcpBindMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MethodRuntimeStatus {
    Stopped,
    Running,
    Unavailable,
    ComingSoon,
}

/// Shared context passed when starting a connection method.
pub struct MethodContext<'a> {
    pub config: &'a RemoteAccessConfig,
    /// Port the shared HTTP listener is bound to (or will bind to).
    pub listen_port: u16,
    /// Bound address string, e.g. `0.0.0.0:4520` or `127.0.0.1:4520`.
    #[allow(dead_code)]
    pub bind_addr: String,
}

#[async_trait]
pub trait ConnectionMethod: Send + Sync {
    fn id(&self) -> &'static str;
    #[allow(dead_code)]
    fn status(&self) -> MethodRuntimeStatus;
    async fn start(&mut self, ctx: &MethodContext<'_>) -> DesktopResult<()>;
    async fn stop(&mut self) -> DesktopResult<()>;
    fn pairing_endpoints(&self, cfg: &RemoteAccessConfig) -> Vec<PairingEndpoint>;
}

/// Build the set of method adapters from current prefs (not yet started).
pub fn build_methods(cfg: &RemoteAccessConfig) -> Vec<Box<dyn ConnectionMethod>> {
    let mut methods: Vec<Box<dyn ConnectionMethod>> = Vec::new();
    if cfg.methods.manual {
        methods.push(Box::new(TcpBindMethod::manual()));
    }
    if cfg.methods.lan {
        methods.push(Box::new(TcpBindMethod::lan()));
    }
    if cfg.methods.public_port {
        methods.push(Box::new(TcpBindMethod::public_port()));
    }
    if cfg.methods.bonjour {
        methods.push(Box::new(BonjourMethod::new()));
    }
    if cfg.methods.cloudflare.enabled {
        methods.push(Box::new(CloudflareMethod::new(
            cfg.methods.cloudflare.hostname.clone(),
        )));
    }
    if cfg.methods.bluetooth {
        methods.push(Box::new(BluetoothMethod::new()));
    }
    methods
}
