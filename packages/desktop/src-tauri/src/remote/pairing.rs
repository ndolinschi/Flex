//! Pairing / connection-info document for mobile clients.

use serde::{Deserialize, Serialize};

use super::config::RemoteAccessConfig;

pub const PROTOCOL_VERSION: u32 = 1;

pub const CAPABILITIES: &[&str] = &[
    "sessions",
    "prompt",
    "events",
    "permissions",
    "questions",
    "mcp",
    "providers",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    #[serde(rename = "type")]
    pub auth_type: String,
    /// Present in Settings pairing payloads; omitted from unauthenticated
    /// responses (there are none for `/v1/info` — it requires the bearer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingEndpoint {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingInfo {
    pub protocol_version: u32,
    pub app_version: String,
    pub device_name: String,
    pub device_id: String,
    pub auth: AuthInfo,
    pub endpoints: Vec<PairingEndpoint>,
    pub capabilities: Vec<String>,
    pub openapi_url: String,
}

impl PairingInfo {
    pub fn build(
        cfg: &RemoteAccessConfig,
        token: &str,
        endpoints: Vec<PairingEndpoint>,
        app_version: &str,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            app_version: app_version.to_owned(),
            device_name: cfg.device_name.clone(),
            device_id: cfg.device_id.clone(),
            auth: AuthInfo {
                auth_type: "bearer".into(),
                token: Some(token.to_owned()),
            },
            endpoints,
            capabilities: CAPABILITIES.iter().map(|s| (*s).to_owned()).collect(),
            openapi_url: "/v1/openapi.json".into(),
        }
    }

    /// Compact JSON suitable for QR / clipboard.
    pub fn to_pairing_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Collect non-loopback IPv4 addresses for LAN pairing URLs.
pub fn lan_ipv4_addrs() -> Vec<String> {
    let Ok(ifaces) = local_ip_address::list_afinet_netifas() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_name, ip) in ifaces {
        if let std::net::IpAddr::V4(v4) = ip {
            if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() {
                let s = v4.to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
    }
    out
}

/// Render pairing JSON as an SVG QR code string (embeddable in the UI).
pub fn pairing_qr_svg(pairing_json: &str) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};

    let code = QrCode::with_error_correction_level(pairing_json.as_bytes(), EcLevel::M)
        .map_err(|e| e.to_string())?;
    Ok(code
        .render::<svg::Color<'_>>()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}
