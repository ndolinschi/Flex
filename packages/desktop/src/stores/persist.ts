import { load } from "@tauri-apps/plugin-store"
import type {
  ComposerMode,
  IsolationPolicy,
  PermissionMode,
  SessionId,
} from "../lib/types"
import type { RightPanelTab, UiTheme, PlanAnnotationsPersisted } from "./types"
import type { ContentLayout } from "./contentLayoutModel"
import { log } from "../lib/debug/log"

export type UiPersisted = {
  activeSessionId: SessionId | null
  selectedModelId?: string | null
  selectedIsolation?: IsolationPolicy | null
  selectedEffort?: string | null
  effortByModel?: Record<string, string>
  composerMode?: ComposerMode
  defaultPermissionMode?: PermissionMode
  theme?: UiTheme
  /** Accent preset (`neutral` default) or `custom`. */
  accentId?: import("../lib/accent").AccentId
  accentCustomHex?: string
  notificationsEnabled?: boolean
  completionSoundEnabled?: boolean
  /** Single app-wide debug-logging switch — see `lib/debug/log.ts`. */
  debugLoggingEnabled?: boolean
  /** Opt-in local crash capture — see `lib/debug/log.ts`. */
  crashReportingEnabled?: boolean
  recentCwds?: string[]
  sidebarCollapsed?: boolean
  sidebarWidth?: number
  /** @deprecated Prefer contentLayout — still read for migration. */
  rightPanelOpen?: boolean
  /** @deprecated Prefer contentLayout. */
  rightPanelTab?: RightPanelTab
  rightPanelWidth?: number
  rightPanelCollapsed?: boolean
  /** @deprecated Prefer contentLayout — still read for migration. */
  openTabsBySession?: Record<string, RightPanelTab[]>
  /** Primary content pane layout (single / split + tabs). */
  contentLayout?: ContentLayout
  browserLastUrl?: string
  pinnedSessionIds?: string[]
  archivedSessionIds?: string[]
  /** Center-pane open chat tabs (session ids), open-order left→right. */
  openChatSessionIds?: string[]
  /** Plan-tab annotations + last-opened plan id, keyed by session id. */
  planAnnotationsBySession?: Record<string, PlanAnnotationsPersisted>
}

export const UI_STORE_FILE = "ui.json"
const UI_KEY = "state"

let storeReady: Promise<void> | null = null
let cachedStore: Awaited<ReturnType<typeof load>> | null = null

const ensureStore = async () => {
  if (!storeReady) {
    storeReady = (async () => {
      cachedStore = await load(UI_STORE_FILE, { autoSave: true, defaults: {} })
    })()
  }
  await storeReady
}

export const persistUiState = async (partial: Partial<UiPersisted>) => {
  try {
    await ensureStore()
    if (!cachedStore) return
    const current = (await cachedStore.get<UiPersisted>(UI_KEY)) ?? {
      activeSessionId: null,
      selectedModelId: null,
      composerMode: "agent" as ComposerMode,
      theme: "dark" as UiTheme,
      recentCwds: [] as string[],
    }
    await cachedStore.set(UI_KEY, { ...current, ...partial })
    await cachedStore.save()
  } catch (err) {
    // Non-fatal — UI still works with in-memory state.
    log.warn("boot", "persistUiState failed", {
      error: err instanceof Error ? err.message : String(err),
    })
  }
}

export const restoreUiState = async (): Promise<UiPersisted> => {
  try {
    await ensureStore()
    if (!cachedStore) {
      return {
        activeSessionId: null,
        selectedModelId: null,
        composerMode: "agent",
        theme: "dark",
        recentCwds: [],
      }
    }
    return (
      (await cachedStore.get<UiPersisted>(UI_KEY)) ?? {
        activeSessionId: null,
        selectedModelId: null,
        composerMode: "agent",
        theme: "dark",
        recentCwds: [],
      }
    )
  } catch (err) {
    log.warn("boot", "restoreUiState failed — using defaults", {
      error: err instanceof Error ? err.message : String(err),
    })
    return {
      activeSessionId: null,
      selectedModelId: null,
      composerMode: "agent",
      theme: "dark",
      recentCwds: [],
    }
  }
}
