import { isBrowserPreview } from "./browserPreview"
import { log } from "./debug/log"

export type UpdateCheckResult =
  | { status: "up-to-date" }
  | { status: "available"; version: string; notes: string | null; date: string | null }
  | { status: "unavailable"; reason: string }
  | { status: "error"; message: string }

const isTauriRuntime = (): boolean => {
  if (isBrowserPreview()) return false
  try {
    return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
  } catch {
    return false
  }
}

export const checkForAppUpdate = async (): Promise<UpdateCheckResult> => {
  if (!isTauriRuntime()) {
    return {
      status: "unavailable",
      reason: "Updates are only available in the packaged desktop app.",
    }
  }

  try {
    const { check } = await import("@tauri-apps/plugin-updater")
    const update = await check()
    if (!update) {
      log.info("boot", "updater: already on latest")
      return { status: "up-to-date" }
    }
    log.info("boot", "updater: update available", { version: update.version })
    return {
      status: "available",
      version: update.version,
      notes: update.body ?? null,
      date: update.date ?? null,
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    const soft =
      /404|not found|failed to fetch|error sending request|signature|pubkey|endpoint|valid release json|release json/i.test(
        message,
      )
    log.warn("boot", "updater: check failed", { message, soft })
    if (soft) {
      return {
        status: "unavailable",
        reason:
          "No update channel yet. Releases need TAURI_SIGNING_PRIVATE_KEY + a published latest.json (and Apple certs for notarized macOS installs).",
      }
    }
    return { status: "error", message }
  }
}

export const installAppUpdateAndRelaunch = async (): Promise<boolean> => {
  if (!isTauriRuntime()) return false

  const { check } = await import("@tauri-apps/plugin-updater")
  const { relaunch } = await import("@tauri-apps/plugin-process")
  const update = await check()
  if (!update) return false

  await update.downloadAndInstall()
  await relaunch()
  return true
}
