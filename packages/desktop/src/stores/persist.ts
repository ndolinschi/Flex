import { load } from "@tauri-apps/plugin-store"
import type {
  ComposerMode,
  IsolationPolicy,
  PermissionMode,
  SessionId,
} from "../lib/types"
import type { RightPanelTab, UiTheme } from "./types"

export type UiPersisted = {
  activeSessionId: SessionId | null
  selectedModelId?: string | null
  selectedIsolation?: IsolationPolicy | null
  selectedEffort?: string | null
  effortByModel?: Record<string, string>
  composerMode?: ComposerMode
  defaultPermissionMode?: PermissionMode
  theme?: UiTheme
  notificationsEnabled?: boolean
  completionSoundEnabled?: boolean
  /** Single app-wide debug-logging switch — see `lib/debug/log.ts`. */
  debugLoggingEnabled?: boolean
  /** Opt-in local crash capture — see `lib/debug/log.ts`. */
  crashReportingEnabled?: boolean
  recentCwds?: string[]
  sidebarCollapsed?: boolean
  sidebarWidth?: number
  rightPanelOpen?: boolean
  rightPanelTab?: RightPanelTab
  rightPanelWidth?: number
  rightPanelCollapsed?: boolean
  /** Per-session set of open right-panel tabs ("Open Tabs") —
   * keyed by `sessionScopeKey`. Restored on boot so a session's open tabs
   * survive an app restart even though the underlying terminal/browser
   * state itself does not (see `openTabsBySession` in stores/types.ts). */
  openTabsBySession?: Record<string, RightPanelTab[]>
  browserLastUrl?: string
  pinnedSessionIds?: string[]
  archivedSessionIds?: string[]
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
  } catch {
    // Non-fatal
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
  } catch {
    return {
      activeSessionId: null,
      selectedModelId: null,
      composerMode: "agent",
      theme: "dark",
      recentCwds: [],
    }
  }
}
