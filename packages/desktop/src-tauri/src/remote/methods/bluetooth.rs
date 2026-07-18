//! Bluetooth connection method — stub for v1 (Coming soon).

use async_trait::async_trait;

use super::{ConnectionMethod, MethodContext, MethodRuntimeStatus};
use crate::error::DesktopResult;
use crate::remote::config::RemoteAccessConfig;
use crate::remote::pairing::PairingEndpoint;

pub struct BluetoothMethod {
    status: MethodRuntimeStatus,
}

impl BluetoothMethod {
    pub fn new() -> Self {
        Self {
            status: MethodRuntimeStatus::ComingSoon,
        }
    }
}

#[async_trait]
impl ConnectionMethod for BluetoothMethod {
    fn id(&self) -> &'static str {
        "bluetooth"
    }

    fn status(&self) -> MethodRuntimeStatus {
        self.status
    }

    async fn start(&mut self, _ctx: &MethodContext<'_>) -> DesktopResult<()> {
        self.status = MethodRuntimeStatus::ComingSoon;
        Ok(())
    }

    async fn stop(&mut self) -> DesktopResult<()> {
        self.status = MethodRuntimeStatus::ComingSoon;
        Ok(())
    }

    fn pairing_endpoints(&self, _cfg: &RemoteAccessConfig) -> Vec<PairingEndpoint> {
        vec![PairingEndpoint {
            method: "bluetooth".into(),
            url: None,
            host: None,
            port: None,
            service_type: None,
            tunnel_hostname: None,
            status: Some("coming_soon".into()),
            note: Some(
                "Bluetooth framing will share the same Remote API handlers in a later release"
                    .into(),
            ),
        }]
    }
}
