//! Bonjour / mDNS discovery for the desktop Remote Access listener.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tokio::sync::Mutex;

use super::{ConnectionMethod, MethodContext, MethodRuntimeStatus};
use crate::error::{DesktopError, DesktopResult};
use crate::remote::config::RemoteAccessConfig;
use crate::remote::pairing::PairingEndpoint;

/// DNS-SD service type clients browse for on the LAN.
pub const BONJOUR_SERVICE_TYPE: &str = "_agentloop-desktop._tcp.local.";

pub struct BonjourMethod {
    status: MethodRuntimeStatus,
    daemon: Option<ServiceDaemon>,
    fullname: Option<String>,
    listen_port: u16,
    /// Last error reason when unavailable.
    note: Arc<Mutex<Option<String>>>,
}

impl BonjourMethod {
    pub fn new() -> Self {
        Self {
            status: MethodRuntimeStatus::Stopped,
            daemon: None,
            fullname: None,
            listen_port: 0,
            note: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl ConnectionMethod for BonjourMethod {
    fn id(&self) -> &'static str {
        "bonjour"
    }

    fn status(&self) -> MethodRuntimeStatus {
        self.status
    }

    async fn start(&mut self, ctx: &MethodContext<'_>) -> DesktopResult<()> {
        let _ = self.stop().await;

        let daemon = ServiceDaemon::new().map_err(|e| {
            DesktopError::Message(format!("bonjour: failed to start mDNS daemon: {e}"))
        })?;

        let host_ipv4 = local_ip_address::local_ip().ok().and_then(|ip| match ip {
            IpAddr::V4(v4) => Some(v4),
            _ => None,
        });

        let Some(host_ipv4) = host_ipv4 else {
            self.status = MethodRuntimeStatus::Unavailable;
            *self.note.lock().await =
                Some("No local IPv4 address for Bonjour advertisement".into());
            return Ok(());
        };

        let instance = sanitize_instance_name(&ctx.config.device_name);
        let mut props = HashMap::new();
        props.insert("device_id".to_owned(), ctx.config.device_id.clone());
        props.insert("path".to_owned(), "/v1".to_owned());
        props.insert("proto".to_owned(), "1".to_owned());
        props.insert("tls".to_owned(), "0".to_owned());

        let info = ServiceInfo::new(
            BONJOUR_SERVICE_TYPE,
            &instance,
            &format!("{host_ipv4}.local."),
            IpAddr::V4(host_ipv4),
            ctx.listen_port,
            Some(props),
        )
        .map_err(|e| DesktopError::Message(format!("bonjour: invalid service info: {e}")))?;

        let fullname = info.get_fullname().to_owned();
        daemon
            .register(info)
            .map_err(|e| DesktopError::Message(format!("bonjour: register failed: {e}")))?;

        self.daemon = Some(daemon);
        self.fullname = Some(fullname);
        self.listen_port = ctx.listen_port;
        self.status = MethodRuntimeStatus::Running;
        *self.note.lock().await = None;
        Ok(())
    }

    async fn stop(&mut self) -> DesktopResult<()> {
        if let (Some(daemon), Some(fullname)) = (self.daemon.take(), self.fullname.take()) {
            let _ = daemon.unregister(&fullname);
            let _ = daemon.shutdown();
        }
        self.listen_port = 0;
        self.status = MethodRuntimeStatus::Stopped;
        Ok(())
    }

    fn pairing_endpoints(&self, cfg: &RemoteAccessConfig) -> Vec<PairingEndpoint> {
        let status = match self.status {
            MethodRuntimeStatus::Running => "running",
            MethodRuntimeStatus::Unavailable => "unavailable",
            MethodRuntimeStatus::ComingSoon => "coming_soon",
            MethodRuntimeStatus::Stopped => return Vec::new(),
        };
        let port = if self.listen_port == 0 {
            cfg.port
        } else {
            self.listen_port
        };
        vec![PairingEndpoint {
            method: "bonjour".into(),
            url: None,
            host: None,
            port: Some(port),
            service_type: Some(BONJOUR_SERVICE_TYPE.trim_end_matches('.').to_owned()),
            tunnel_hostname: None,
            status: Some(status.into()),
            note: Some(format!(
                "Browse {BONJOUR_SERVICE_TYPE} — TXT device_id={}",
                cfg.device_id
            )),
        }]
    }
}

fn sanitize_instance_name(name: &str) -> String {
    let trimmed = name.trim();
    let mut out = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == ' ' {
            out.push(ch);
        }
    }
    let out = out.trim().replace(' ', "-");
    if out.is_empty() {
        "desktop".into()
    } else {
        out.chars().take(63).collect()
    }
}
