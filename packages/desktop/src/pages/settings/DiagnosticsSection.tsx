import { useEffect, useState } from "react"
import { Check, Copy, FolderOpen } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { Toggle } from "../../components/atoms"
import { SettingsCard, SettingRow } from "../../components/molecules"
import { exportDebugLog, exportDiagnostics, log } from "../../lib/debug/log"
import {
  checkForAppUpdate,
  installAppUpdateAndRelaunch,
  type UpdateCheckResult,
} from "../../lib/updater"
import { appVersion, debugLogPath, toInvokeError } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"

/** Diagnostics/Developer section: app-wide debug logging, opt-in local crash
 * capture, and "Export diagnostics" (frontend rings + backend log tail —
 * no session required). Remote crash upload (Sentry DSN) is intentionally
 * not wired. */
export const DiagnosticsContent = () => {
  const debugLoggingEnabled = useAppStore((s) => s.debugLoggingEnabled)
  const setDebugLoggingEnabled = useAppStore((s) => s.setDebugLoggingEnabled)
  const crashReportingEnabled = useAppStore((s) => s.crashReportingEnabled)
  const setCrashReportingEnabled = useAppStore((s) => s.setCrashReportingEnabled)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const pushToast = useAppStore((s) => s.pushToast)

  const [exporting, setExporting] = useState(false)
  const [exportingSession, setExportingSession] = useState(false)
  const [copied, setCopied] = useState(false)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [updateStatus, setUpdateStatus] = useState<string | null>(null)
  const [version, setVersion] = useState<string | null>(null)

  useEffect(() => {
    void appVersion().then(setVersion).catch(() => setVersion(null))
  }, [])

  const handleExportDiagnostics = async () => {
    setExporting(true)
    try {
      const path = await exportDiagnostics()
      pushToast(`Diagnostics saved to ${path}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      log.error("boot", "export diagnostics failed", { error: message })
      pushToast(message, "error")
    } finally {
      setExporting(false)
    }
  }

  const handleExportSessionLog = async () => {
    setExportingSession(true)
    try {
      const path = await exportDebugLog()
      pushToast(`Debug log saved to ${path}`, "success")
    } catch (err) {
      const message = toInvokeError(err)
      log.error("boot", "export session debug log failed", { error: message })
      pushToast(message, "error")
    } finally {
      setExportingSession(false)
    }
  }

  const handleCopyLogPath = async () => {
    try {
      const path = await debugLogPath()
      await navigator.clipboard.writeText(path)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch (err) {
      const message = toInvokeError(err)
      log.error("boot", "copy log path failed", { error: message })
      pushToast(message, "error")
    }
  }

  const handleOpenLogsFolder = async () => {
    try {
      const path = await debugLogPath()
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener")
      await revealItemInDir(path)
    } catch (err) {
      const message = toInvokeError(err)
      log.error("boot", "open logs folder failed", { error: message })
      pushToast(message, "error")
    }
  }

  const describeUpdate = (result: UpdateCheckResult): string => {
    switch (result.status) {
      case "up-to-date":
        return "You're on the latest version."
      case "available":
        return `Update ${result.version} is available.`
      case "unavailable":
        return result.reason
      case "error":
        return result.message
    }
  }

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true)
    setUpdateStatus(null)
    try {
      const result = await checkForAppUpdate()
      setUpdateStatus(describeUpdate(result))
      if (result.status === "available") {
        pushToast(`Update ${result.version} available`, "success", {
          label: "Install",
          onAction: () => {
            void installAppUpdateAndRelaunch().catch((err) => {
              pushToast(toInvokeError(err), "error")
            })
          },
        })
      }
    } catch (err) {
      setUpdateStatus(toInvokeError(err))
    } finally {
      setCheckingUpdate(false)
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <SettingsCard label="Debug logging">
        <SettingRow
          rowId="diagnostics-debug-logging"
          title="Debug logging"
          description="Warn/error always recorded for production. Debug/info and raw session events only when this is on. Backend log level rises on next launch."
          first
        >
          <Toggle
            checked={debugLoggingEnabled}
            onChange={setDebugLoggingEnabled}
            label="Toggle debug logging"
          />
        </SettingRow>
        <SettingRow
          rowId="diagnostics-crash-reporting"
          title="Crash reporting (local)"
          description="Retain uncaught errors in memory for the diagnostics export. Nothing is uploaded — remote reporting (Sentry DSN) is not configured."
        >
          <Toggle
            checked={crashReportingEnabled}
            onChange={setCrashReportingEnabled}
            label="Toggle local crash capture"
          />
        </SettingRow>
        <SettingRow
          rowId="diagnostics-export"
          title="Export diagnostics"
          description="Save frontend logs, crash ring, raw session events, and a backend log tail into the app log folder. Works without an open session."
        >
          <Button
            variant="secondary"
            size="sm"
            disabled={exporting}
            onClick={() => void handleExportDiagnostics()}
          >
            {exporting ? <Spinner data-icon="inline-start" /> : null}
            Export diagnostics
          </Button>
        </SettingRow>
        <SettingRow
          rowId="diagnostics-export-session"
          title="Export debug log to workspace"
          description={
            activeSessionId
              ? "Also write the frontend debug payload into the active session's workspace."
              : "Open a session first — this export is saved into its workspace."
          }
        >
          <Button
            variant="ghost"
            size="sm"
            disabled={!activeSessionId || exportingSession}
            onClick={() => void handleExportSessionLog()}
          >
            {exportingSession ? <Spinner data-icon="inline-start" /> : null}
            Export to workspace
          </Button>
        </SettingRow>
        <SettingRow
          rowId="diagnostics-backend-log"
          title="Backend log file"
          description="The native process also writes a rolling log file to disk — useful for packaged builds where the DevTools console isn't available."
        >
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={() => void handleCopyLogPath()}>
              {copied ? (
                <Check className="h-3.5 w-3.5" aria-hidden />
              ) : (
                <Copy className="h-3.5 w-3.5" aria-hidden />
              )}
              {copied ? "Copied" : "Copy log path"}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => void handleOpenLogsFolder()}>
              <FolderOpen className="h-3.5 w-3.5" aria-hidden />
              Open logs folder
            </Button>
          </div>
        </SettingRow>
      </SettingsCard>

      <SettingsCard label="Updates">
        <SettingRow
          rowId="diagnostics-updates"
          title="Check for updates"
          description={
            version
              ? `Current version ${version}. Channel: GitHub Releases (latest.json). Signing + Apple notarization still need secrets (see release.yml TODOs).`
              : "Channel: GitHub Releases (latest.json). Signing + Apple notarization still need secrets."
          }
          first
        >
          <div className="flex flex-col items-end gap-1.5">
            <Button
              variant="secondary"
              size="sm"
              disabled={checkingUpdate}
              onClick={() => void handleCheckUpdate()}
            >
              {checkingUpdate ? <Spinner data-icon="inline-start" /> : null}
              Check for updates
            </Button>
            {updateStatus ? (
              <p className="max-w-xs text-right text-xs text-ink-muted">{updateStatus}</p>
            ) : null}
          </div>
        </SettingRow>
      </SettingsCard>
    </div>
  )
}
