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
  /** Session-sidebar repository sort (`recency` | `alpha`). */
  sidebarProjectSort?: import("../lib/sessionGrouping").SidebarProjectSort
  /** Session-sidebar repository visibility (`active` | `all`). */
  sidebarProjectVisibility?: import("../lib/sessionGrouping").SidebarProjectVisibility
  /** Center-pane open chat tabs (session ids), open-order left→right. */
  openChatSessionIds?: string[]
  /** Plan-tab annotations + last-opened plan id, keyed by session id. */
  planAnnotationsBySession?: Record<string, PlanAnnotationsPersisted>
  /** Active custom theme id, or `"factory"` for the built-in palette. */
  activeThemeId?: string
  /** User-defined named themes (allowlisted token overrides). */
  customThemes?: import("../lib/themeTokens").ThemeSpec[]
}

export const UI_STORE_FILE = "ui.json"
const UI_KEY = "state"

/** Coalesce rapid tab/chat switches into one disk write. */
const PERSIST_DEBOUNCE_MS = 280

let storeReady: Promise<void> | null = null
let cachedStore: Awaited<ReturnType<typeof load>> | null = null
let pendingPartial: Partial<UiPersisted> = {}
let persistTimer: ReturnType<typeof setTimeout> | null = null
let flushChain: Promise<void> = Promise.resolve()

const ensureStore = async () => {
  if (!storeReady) {
    storeReady = (async () => {
      // autoSave off — we own coalesced explicit saves so tab switches don't
      // each hit the disk (plugin-store set+save was a noticeable UI hitch).
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
    // Non-fatal — UI still works with in-memory state.
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

/** Persist UI prefs. Debounced — safe to call on every tab/chat switch. */
export const persistUiState = async (partial: Partial<UiPersisted>) => {
  pendingPartial = { ...pendingPartial, ...partial }
  scheduleFlush()
}

/** Flush any pending write immediately (tests / before quit). */
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
