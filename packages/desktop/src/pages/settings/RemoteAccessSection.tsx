import { useCallback, useEffect, useState } from "react"
import { Check, Copy, RefreshCw } from "lucide-react"
import { Button, TextInput, Toggle } from "../../components/atoms"
import { SettingsCard, SettingRow } from "../../components/molecules"
import {
  remoteAccessGet,
  remoteAccessRotateToken,
  remoteAccessSave,
  toInvokeError,
  type MethodPrefs,
  type RemoteAccessStatus,
} from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"

const METHOD_LABELS: Record<keyof MethodPrefs, { title: string; description: string }> = {
  manual: {
    title: "Manual",
    description: "Show host:port + token (or QR). Client pastes or scans them.",
  },
  lan: {
    title: "LAN",
    description: "Bind on all interfaces and advertise this machine’s LAN IPs.",
  },
  bonjour: {
    title: "Bonjour / mDNS",
    description: "Advertise _agentloop-desktop._tcp so clients can discover this desktop on the LAN.",
  },
  publicPort: {
    title: "Public port",
    description: "Expose the listener on the chosen port for WAN/NAT — use a strong token.",
  },
  cloudflare: {
    title: "Cloudflare Tunnel",
    description: "Requires cloudflared on PATH. Spawns a quick tunnel to the local listener.",
  },
  bluetooth: {
    title: "Bluetooth",
    description: "Coming soon — same Remote API over a Bluetooth pipe.",
  },
}

/** Settings → Remote Access: enable the desktop-owned HTTP/SSE transport and
 * choose connection methods (manual / LAN / Bonjour / public port / Cloudflare
 * stub / Bluetooth stub). Pairing panel exposes what a mobile client needs. */
export const RemoteAccessContent = () => {
  const pushToast = useAppStore((s) => s.pushToast)
  const [status, setStatus] = useState<RemoteAccessStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [deviceName, setDeviceName] = useState("")
  const [port, setPort] = useState("4520")
  const [tokenVisible, setTokenVisible] = useState(false)
  const [copied, setCopied] = useState<"token" | "json" | "url" | null>(null)

  const refresh = useCallback(async () => {
    try {
      const next = await remoteAccessGet()
      setStatus(next)
      setDeviceName(next.config.deviceName)
      setPort(String(next.config.port))
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    } finally {
      setLoading(false)
    }
  }, [pushToast])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const save = async (patch: {
    enabled?: boolean
    methods?: MethodPrefs
    deviceName?: string
    port?: number
  }) => {
    if (!status) return
    setSaving(true)
    try {
      const next = await remoteAccessSave({
        enabled: patch.enabled ?? status.config.enabled,
        deviceName: patch.deviceName ?? deviceName,
        port: patch.port ?? (Number(port) || status.config.port),
        methods: patch.methods ?? status.config.methods,
      })
      setStatus(next)
      setDeviceName(next.config.deviceName)
      setPort(String(next.config.port))
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    } finally {
      setSaving(false)
    }
  }

  const copy = async (kind: "token" | "json" | "url", value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(kind)
      window.setTimeout(() => setCopied(null), 1500)
    } catch {
      pushToast("Could not copy to clipboard", "error")
    }
  }

  const rotateToken = async () => {
    setSaving(true)
    try {
      const next = await remoteAccessRotateToken()
      setStatus(next)
      pushToast("Remote access token rotated", "success")
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    } finally {
      setSaving(false)
    }
  }

  if (loading || !status) {
    return (
      <p className="text-sm text-ink-muted">Loading remote access…</p>
    )
  }

  const methods = status.config.methods
  const primaryUrl =
    status.pairing?.endpoints.find((e) => e.url)?.url ??
    (status.bindAddr ? `http://${status.bindAddr}` : null)

  return (
    <div className="flex flex-col gap-3">
      <SettingsCard label="Remote Access">
        <SettingRow
          rowId="remote-enabled"
          title="Enable remote access"
          description="Start the desktop Remote API so a mobile client can connect to this app (not a separate engine server)."
          first
        >
          <Toggle
            checked={status.config.enabled}
            disabled={saving}
            onChange={(enabled) => void save({ enabled })}
            label="Toggle remote access"
          />
        </SettingRow>
        <SettingRow
          rowId="remote-device-name"
          title="Device name"
          description="Shown in pairing info and Bonjour advertisement."
          stacked
        >
          <div className="flex items-center gap-2">
            <TextInput
              value={deviceName}
              onChange={(e) => setDeviceName(e.target.value)}
              aria-label="Device name"
              className="max-w-xs"
            />
            <Button
              size="sm"
              variant="secondary"
              disabled={saving || deviceName.trim() === status.config.deviceName}
              onClick={() => void save({ deviceName })}
            >
              Save
            </Button>
          </div>
        </SettingRow>
        <SettingRow
          rowId="remote-port"
          title="Port"
          description="TCP port for the shared HTTP listener (default 4520)."
          stacked
        >
          <div className="flex items-center gap-2">
            <TextInput
              value={port}
              onChange={(e) => setPort(e.target.value.replace(/[^\d]/g, ""))}
              aria-label="Port"
              className="w-24"
            />
            <Button
              size="sm"
              variant="secondary"
              disabled={saving || Number(port) === status.config.port}
              onClick={() => void save({ port: Number(port) || 4520 })}
            >
              Save
            </Button>
          </div>
        </SettingRow>
        <SettingRow
          rowId="remote-status"
          title="Status"
          description={
            status.running
              ? `Listening on ${status.bindAddr ?? "unknown"}`
              : "Server stopped"
          }
        >
          <Button
            size="sm"
            variant="secondary"
            disabled={saving}
            onClick={() => void refresh()}
          >
            <RefreshCw className="h-3.5 w-3.5" aria-hidden />
            Refresh
          </Button>
        </SettingRow>
      </SettingsCard>

      <SettingsCard label="Connection methods">
        {(
          [
            "manual",
            "lan",
            "bonjour",
            "publicPort",
            "cloudflare",
            "bluetooth",
          ] as const
        ).map((key, index) => {
          const meta = METHOD_LABELS[key]
          const checked =
            key === "cloudflare"
              ? methods.cloudflare.enabled
              : Boolean(methods[key])
          const comingSoon = key === "bluetooth"
          return (
            <SettingRow
              key={key}
              rowId={`remote-method-${key}`}
              title={meta.title}
              description={meta.description}
              first={index === 0}
            >
              <Toggle
                checked={checked}
                disabled={saving || comingSoon}
                onChange={(next) => {
                  if (key === "cloudflare") {
                    void save({
                      methods: {
                        ...methods,
                        cloudflare: { ...methods.cloudflare, enabled: next },
                      },
                    })
                  } else if (key !== "bluetooth") {
                    void save({ methods: { ...methods, [key]: next } })
                  }
                }}
                label={`Toggle ${meta.title}`}
              />
            </SettingRow>
          )
        })}
      </SettingsCard>

      {status.config.enabled && (
        <SettingsCard label="Pairing">
          <SettingRow
            rowId="remote-pairing-url"
            title="Endpoint"
            description="Primary URL a client should open (Bearer token required)."
            first
            stacked
          >
            <div className="flex items-center gap-2">
              <code className="flex-1 truncate rounded-md border border-border bg-surface px-2.5 py-1.5 font-mono text-xs text-ink">
                {primaryUrl ?? "—"}
              </code>
              {primaryUrl && (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => void copy("url", primaryUrl)}
                >
                  {copied === "url" ? (
                    <Check className="h-3.5 w-3.5" aria-hidden />
                  ) : (
                    <Copy className="h-3.5 w-3.5" aria-hidden />
                  )}
                  Copy
                </Button>
              )}
            </div>
          </SettingRow>
          <SettingRow
            rowId="remote-pairing-token"
            title="Bearer token"
            description="Authorization: Bearer &lt;token&gt; on every route except /health."
            stacked
          >
            <div className="flex flex-col gap-2">
              <code className="break-all rounded-md border border-border bg-surface px-2.5 py-1.5 font-mono text-xs text-ink">
                {tokenVisible
                  ? status.token ?? "—"
                  : status.token
                    ? "••••••••••••••••"
                    : "—"}
              </code>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => setTokenVisible((v) => !v)}
                >
                  {tokenVisible ? "Hide" : "Reveal"}
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={!status.token}
                  onClick={() => status.token && void copy("token", status.token)}
                >
                  {copied === "token" ? (
                    <Check className="h-3.5 w-3.5" aria-hidden />
                  ) : (
                    <Copy className="h-3.5 w-3.5" aria-hidden />
                  )}
                  Copy
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={saving}
                  onClick={() => void rotateToken()}
                >
                  Rotate
                </Button>
              </div>
            </div>
          </SettingRow>
          <SettingRow
            rowId="remote-pairing-json"
            title="Pairing document"
            description="Versioned JSON a mobile client needs (protocol, device, auth, endpoints, capabilities)."
            stacked
          >
            <div className="flex flex-col gap-2">
              <pre className="max-h-40 overflow-auto rounded-md border border-border bg-surface px-2.5 py-1.5 font-mono text-[11px] leading-relaxed text-ink">
                {status.pairingJson
                  ? JSON.stringify(JSON.parse(status.pairingJson), null, 2)
                  : "—"}
              </pre>
              <Button
                size="sm"
                variant="secondary"
                disabled={!status.pairingJson}
                onClick={() =>
                  status.pairingJson && void copy("json", status.pairingJson)
                }
              >
                {copied === "json" ? (
                  <Check className="h-3.5 w-3.5" aria-hidden />
                ) : (
                  <Copy className="h-3.5 w-3.5" aria-hidden />
                )}
                Copy JSON
              </Button>
            </div>
          </SettingRow>
          {status.pairingQrSvg && (
            <SettingRow
              rowId="remote-pairing-qr"
              title="QR code"
              description="Scan with a mobile client to load the pairing document."
              stacked
            >
              <div
                className="inline-block rounded-md border border-border bg-white p-2"
                // SVG generated by the Rust qrcode crate from pairing JSON.
                dangerouslySetInnerHTML={{ __html: status.pairingQrSvg }}
              />
            </SettingRow>
          )}
          {status.methodNotes.length > 0 && (
            <SettingRow
              rowId="remote-method-notes"
              title="Method details"
              description="Per-method status from the running adapters."
              stacked
            >
              <ul className="flex flex-col gap-1.5 text-xs text-ink-muted">
                {status.methodNotes.map((note) => (
                  <li key={`${note.id}-${note.status}`}>
                    <span className="font-medium text-ink">{note.id}</span>
                    {" — "}
                    {note.status}
                    {note.note ? `: ${note.note}` : ""}
                  </li>
                ))}
              </ul>
            </SettingRow>
          )}
        </SettingsCard>
      )}
    </div>
  )
}
