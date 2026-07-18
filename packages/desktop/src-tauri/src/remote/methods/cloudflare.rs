//! Cloudflare Tunnel adapter — thin supervisor stub for v1.
//!
//! When `cloudflared` is on PATH and the method is enabled, attempts a quick
//! tunnel to the local listener. Named tunnels / full account wiring is later.

use std::process::Stdio;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::{ConnectionMethod, MethodContext, MethodRuntimeStatus};
use crate::error::DesktopResult;
use crate::remote::config::RemoteAccessConfig;
use crate::remote::pairing::PairingEndpoint;

pub struct CloudflareMethod {
    status: MethodRuntimeStatus,
    hostname_pref: Option<String>,
    tunnel_hostname: Option<String>,
    child: Mutex<Option<Child>>,
    note: Option<String>,
    listen_port: u16,
}

impl CloudflareMethod {
    pub fn new(hostname_pref: Option<String>) -> Self {
        Self {
            status: MethodRuntimeStatus::Stopped,
            hostname_pref,
            tunnel_hostname: None,
            child: Mutex::new(None),
            note: None,
            listen_port: 0,
        }
    }
}

#[async_trait]
impl ConnectionMethod for CloudflareMethod {
    fn id(&self) -> &'static str {
        "cloudflare"
    }

    fn status(&self) -> MethodRuntimeStatus {
        self.status
    }

    async fn start(&mut self, ctx: &MethodContext<'_>) -> DesktopResult<()> {
        let _ = self.stop().await;
        self.listen_port = ctx.listen_port;

        let cloudflared = which_cloudflared();
        let Some(bin) = cloudflared else {
            self.status = MethodRuntimeStatus::Unavailable;
            self.note = Some(
                "cloudflared not found on PATH — install Cloudflare Tunnel to enable this method"
                    .into(),
            );
            return Ok(());
        };

        // Named hostname preference is Phase C; v1 uses quick tunnels only.
        let _ = &self.hostname_pref;

        let mut child = Command::new(&bin)
            .args([
                "tunnel",
                "--url",
                &format!("http://127.0.0.1:{}", ctx.listen_port),
                "--no-autoupdate",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                crate::error::DesktopError::Message(format!("cloudflare: spawn failed: {e}"))
            })?;

        // Best-effort: scrape stderr for the trycloudflare.com URL.
        if let Some(stderr) = child.stderr.take() {
            let note_slot = std::sync::Arc::new(tokio::sync::Mutex::new(None::<String>));
            let note_clone = note_slot.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(url) = extract_trycloudflare_url(&line) {
                        *note_clone.lock().await = Some(url);
                        break;
                    }
                }
            });
            // Give the tunnel a moment to print the URL.
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            self.tunnel_hostname = note_slot.lock().await.clone();
        }

        *self.child.lock().await = Some(child);
        if self.tunnel_hostname.is_some() {
            self.status = MethodRuntimeStatus::Running;
            self.note = None;
        } else {
            self.status = MethodRuntimeStatus::Running;
            self.note = Some(
                "cloudflared started — waiting for trycloudflare.com URL in process output".into(),
            );
        }
        Ok(())
    }

    async fn stop(&mut self) -> DesktopResult<()> {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
        self.tunnel_hostname = None;
        self.listen_port = 0;
        self.status = MethodRuntimeStatus::Stopped;
        self.note = None;
        Ok(())
    }

    fn pairing_endpoints(&self, _cfg: &RemoteAccessConfig) -> Vec<PairingEndpoint> {
        if matches!(
            self.status,
            MethodRuntimeStatus::Stopped | MethodRuntimeStatus::ComingSoon
        ) {
            return Vec::new();
        }
        let status = match self.status {
            MethodRuntimeStatus::Running => "running",
            MethodRuntimeStatus::Unavailable => "unavailable",
            _ => "stopped",
        };
        let url = self.tunnel_hostname.as_ref().map(|h| {
            if h.starts_with("http") {
                h.clone()
            } else {
                format!("https://{h}")
            }
        });
        vec![PairingEndpoint {
            method: "cloudflare".into(),
            url: url.clone(),
            host: self.tunnel_hostname.clone(),
            port: Some(443),
            service_type: None,
            tunnel_hostname: self.tunnel_hostname.clone(),
            status: Some(status.into()),
            note: self.note.clone().or_else(|| {
                if url.is_none() {
                    Some("Requires cloudflared on PATH".into())
                } else {
                    None
                }
            }),
        }]
    }
}

fn which_cloudflared() -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("cloudflared");
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let candidate = dir.join("cloudflared.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    })
}

fn extract_trycloudflare_url(line: &str) -> Option<String> {
    // cloudflared prints something like:
    //   https://random-words.trycloudflare.com
    for part in line.split_whitespace() {
        if part.starts_with("https://") && part.contains("trycloudflare.com") {
            return Some(part.trim_matches(|c| c == '"' || c == '\'').to_owned());
        }
    }
    None
}
