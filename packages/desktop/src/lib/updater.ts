// Auto-update helpers (tauri-plugin-updater).
//
// Wired for the GitHub Releases channel (`latest.json` at the repo's latest
// release). Signing requires `TAURI_SIGNING_PRIVATE_KEY` in CI (see
// `.github/workflows/release.yml`); Apple notarization is still blocked on
// Developer certs (phase 2.2). Until a signed release exists the endpoint
// 404s and `checkForAppUpdate` returns `{ status: "unavailable" }` — that is
// expected, not an error.
//
// Browser preview / Vite-only runs never load the native plugin — every
// helper short-circuits to a safe stub.
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

/** Poll the configured updater endpoint. Never throws — callers surface
 * `status`/`reason` in Settings or a toast. */
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
    // Missing latest.json / network / unsigned channel — common until the
    // first signed release lands. Treat as "unavailable" so Settings stays
    // calm rather than red.
    const soft =
      /404|not found|failed to fetch|error sending request|signature|pubkey|endpoint/i.test(
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

/** Download + install an available update, then relaunch. Returns false when
 * nothing was installed (already current / plugin unavailable). */
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
