
use async_trait::async_trait;

use super::{ConnectionMethod, MethodContext, MethodRuntimeStatus};
use crate::error::DesktopResult;
use crate::remote::config::RemoteAccessConfig;
use crate::remote::pairing::{lan_ipv4_addrs, PairingEndpoint};

#[derive(Debug, Clone, Copy)]
pub enum TcpKind {
    Manual,
    Lan,
    PublicPort,
}

pub struct TcpBindMethod {
    kind: TcpKind,
    status: MethodRuntimeStatus,
    listen_port: u16,
}

impl TcpBindMethod {
    pub fn manual() -> Self {
        Self {
            kind: TcpKind::Manual,
            status: MethodRuntimeStatus::Stopped,
            listen_port: 0,
        }
    }

    pub fn lan() -> Self {
        Self {
            kind: TcpKind::Lan,
            status: MethodRuntimeStatus::Stopped,
            listen_port: 0,
        }
    }

    pub fn public_port() -> Self {
        Self {
            kind: TcpKind::PublicPort,
            status: MethodRuntimeStatus::Stopped,
            listen_port: 0,
        }
    }
}

#[async_trait]
impl ConnectionMethod for TcpBindMethod {
    fn id(&self) -> &'static str {
        match self.kind {
            TcpKind::Manual => "manual",
            TcpKind::Lan => "lan",
            TcpKind::PublicPort => "public_port",
        }
    }

    fn status(&self) -> MethodRuntimeStatus {
        self.status
    }

    async fn start(&mut self, ctx: &MethodContext<'_>) -> DesktopResult<()> {
        self.listen_port = ctx.listen_port;
        self.status = MethodRuntimeStatus::Running;
        Ok(())
    }

    async fn stop(&mut self) -> DesktopResult<()> {
        self.status = MethodRuntimeStatus::Stopped;
        self.listen_port = 0;
        Ok(())
    }

    fn pairing_endpoints(&self, cfg: &RemoteAccessConfig) -> Vec<PairingEndpoint> {
        if self.status != MethodRuntimeStatus::Running {
            return Vec::new();
        }
        let port = if self.listen_port == 0 {
            cfg.port
        } else {
            self.listen_port
        };
        match self.kind {
            TcpKind::Manual => vec![PairingEndpoint {
                method: "manual".into(),
                url: Some(format!("http://127.0.0.1:{port}")),
                host: Some("127.0.0.1".into()),
                port: Some(port),
                service_type: None,
                tunnel_hostname: None,
                status: Some("running".into()),
                note: Some("Loopback — same machine only".into()),
            }],
            TcpKind::Lan => {
                let addrs = lan_ipv4_addrs();
                if addrs.is_empty() {
                    return vec![PairingEndpoint {
                        method: "lan".into(),
                        url: None,
                        host: None,
                        port: Some(port),
                        service_type: None,
                        tunnel_hostname: None,
                        status: Some("running".into()),
                        note: Some("No LAN IPv4 address detected".into()),
                    }];
                }
                addrs
                    .into_iter()
                    .map(|host| PairingEndpoint {
                        method: "lan".into(),
                        url: Some(format!("http://{host}:{port}")),
                        host: Some(host),
                        port: Some(port),
                        service_type: None,
                        tunnel_hostname: None,
                        status: Some("running".into()),
                        note: None,
                    })
                    .collect()
            }
            TcpKind::PublicPort => vec![PairingEndpoint {
                method: "public_port".into(),
                url: Some(format!("http://0.0.0.0:{port}")),
                host: Some("0.0.0.0".into()),
                port: Some(port),
                service_type: None,
                tunnel_hostname: None,
                status: Some("running".into()),
                note: Some(
                    "Bound on all interfaces — ensure firewall / NAT forwards this port".into(),
                ),
            }],
        }
    }
}
