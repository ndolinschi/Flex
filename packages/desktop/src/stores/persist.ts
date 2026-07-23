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
  accentId?: import("../lib/accent").AccentId
  accentCustomHex?: string
  notificationsEnabled?: boolean
  completionSoundEnabled?: boolean
  debugLoggingEnabled?: boolean
  crashReportingEnabled?: boolean
  recentCwds?: string[]
  sidebarCollapsed?: boolean
  sidebarWidth?: number
  rightPanelOpen?: boolean
  rightPanelTab?: RightPanelTab
  rightPanelWidth?: number
  rightPanelCollapsed?: boolean
  openTabsBySession?: Record<string, RightPanelTab[]>
  contentLayout?: ContentLayout
  browserLastUrl?: string
  pinnedSessionIds?: string[]
  archivedSessionIds?: string[]
  sidebarProjectSort?: import("../lib/sessionGrouping").SidebarProjectSort
  sidebarProjectVisibility?: import("../lib/sessionGrouping").SidebarProjectVisibility
  openChatSessionIds?: string[]
  planAnnotationsBySession?: Record<string, PlanAnnotationsPersisted>
  activeThemeId?: string
  customThemes?: import("../lib/themeTokens").ThemeSpec[]
}

export const UI_STORE_FILE = "ui.json"
const UI_KEY = "state"

const PERSIST_DEBOUNCE_MS = 280

let storeReady: Promise<void> | null = null
let cachedStore: Awaited<ReturnType<typeof load>> | null = null
let pendingPartial: Partial<UiPersisted> = {}
let persistTimer: ReturnType<typeof setTimeout> | null = null
let flushChain: Promise<void> = Promise.resolve()

const ensureStore = async () => {
  if (!storeReady) {
    storeReady = (async () => {
      cachedStore = await load(UI_STORE_FILE, { autoSave: false, defaults: {} })
    })()
  }
  await storeReady
}

const flushPersist = async (partial: Partial<UiPersisted>) => {
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
    log.warn("boot", "persistUiState failed", {
      error: err instanceof Error ? err.message : String(err),
    })
  }
}

const scheduleFlush = () => {
  if (persistTimer !== null) return
  persistTimer = setTimeout(() => {
    persistTimer = null
    const toWrite = pendingPartial
    pendingPartial = {}
    if (Object.keys(toWrite).length === 0) return
    flushChain = flushChain
      .then(() => flushPersist(toWrite))
      .catch(() => undefined)
  }, PERSIST_DEBOUNCE_MS)
}

export const persistUiState = async (partial: Partial<UiPersisted>) => {
  pendingPartial = { ...pendingPartial, ...partial }
  scheduleFlush()
}

export const flushPersistUiState = async () => {
  if (persistTimer !== null) {
    clearTimeout(persistTimer)
    persistTimer = null
  }
  const toWrite = pendingPartial
  pendingPartial = {}
  if (Object.keys(toWrite).length > 0) {
    flushChain = flushChain
      .then(() => flushPersist(toWrite))
      .catch(() => undefined)
  }
  await flushChain
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
