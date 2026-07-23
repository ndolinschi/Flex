import type { StateCreator } from "zustand"
import type { AppState, UiSliceState, UiTheme } from "../types"
import type { AccentId } from "../../lib/accent"
import {
  applyAccentToDom,
  DEFAULT_ACCENT_ID,
  DEFAULT_CUSTOM_ACCENT,
  normalizeAccentHex,
} from "../../lib/accent"
import {
  applyThemeTokensToDom,
  clearThemeTokensFromDom,
  parseThemeJson,
} from "../../lib/themeTokens"
import type { ThemeSpec } from "../../lib/themeTokens"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import { persistUiState } from "../persist"
import { syncCrashReportingFlag, syncDebugFlag } from "../../lib/debug/log"
import { toast } from "sonner"

const FACTORY_THEME_ID = "factory"

const applyThemeToDom = (theme: UiTheme) => {
  if (typeof document === "undefined") return
  document.documentElement.setAttribute("data-theme", theme)
}

const syncAccentToDom = (state: {
  accentId: AccentId
  accentCustomHex: string
  theme: UiTheme
}) => {
  applyAccentToDom(state.accentId, state.accentCustomHex, state.theme)
}

const syncCustomThemeToDom = (
  activeThemeId: string,
  customThemes: ThemeSpec[],
  mode: UiTheme,
): void => {
  if (activeThemeId === FACTORY_THEME_ID) {
    clearThemeTokensFromDom()
    return
  }
  const spec = customThemes.find((t) => t.id === activeThemeId)
  if (!spec) {
    clearThemeTokensFromDom()
    return
  }
  applyThemeTokensToDom(mode, spec.tokens?.[mode])
}

let toastCounter = 0

export const createUiSlice: StateCreator<
  AppState,
  [],
  [],
  UiSliceState
> = (set, get) => ({
  route: "welcome",
  settingsSection: "general",
  theme: "dark",
  accentId: DEFAULT_ACCENT_ID,
  accentCustomHex: DEFAULT_CUSTOM_ACCENT,
  notificationsEnabled: true,
  completionSoundEnabled: false,
  debugLoggingEnabled: false,
  crashReportingEnabled: false,
  isBootstrapped: false,
  recentCwds: [],
  pinnedSessionIds: [],
  archivedSessionIds: [],
  sidebarProjectSort: "recency",
  sidebarProjectVisibility: "all",
  openChatSessionIds: [],
  unreadBySession: {},
  toasts: [],
  activeThemeId: FACTORY_THEME_ID,
  customThemes: [],
  setRoute: (route) =>
    set((state) => {
      const effectiveRoute =
        route === "automations" && !AUTOMATIONS_UI_ENABLED ? "settings" : route

      let settingsSection = state.settingsSection
      if (effectiveRoute === "memory") {
        settingsSection = "memory"
      } else if (route === "automations" && AUTOMATIONS_UI_ENABLED) {
        settingsSection = "automations"
      } else if (effectiveRoute === "customize") {
        settingsSection = "tools-mcp"
      }
      if (settingsSection === "automations" && !AUTOMATIONS_UI_ENABLED) {
        settingsSection = "general"
      }

      return {
        route: effectiveRoute,
        subagentViewer: effectiveRoute === "chat" ? state.subagentViewer : null,
        settingsSection,
      }
    }),
  setSettingsSection: (section) => {
    if (section === "automations" && !AUTOMATIONS_UI_ENABLED) {
      set({ settingsSection: "general" })
      return
    }
    set({ settingsSection: section })
  },
  setTheme: (theme) => {
    applyThemeToDom(theme)
    set({ theme })
    void persistUiState({ theme })
    const { accentId, accentCustomHex, activeThemeId, customThemes } = get()
    syncAccentToDom({ accentId, accentCustomHex, theme })
    syncCustomThemeToDom(activeThemeId, customThemes, theme)
  },
  toggleTheme: () => {
    const next = get().theme === "dark" ? "light" : "dark"
    get().setTheme(next)
  },
  setAccentId: (id) => {
    set({ accentId: id })
    void persistUiState({ accentId: id })
    const { accentCustomHex, theme } = get()
    syncAccentToDom({ accentId: id, accentCustomHex, theme })
  },
  setAccentCustomHex: (hex) => {
    const normalized = normalizeAccentHex(hex)
    if (!normalized) return
    set({ accentId: "custom", accentCustomHex: normalized })
    void persistUiState({ accentId: "custom", accentCustomHex: normalized })
    syncAccentToDom({
      accentId: "custom",
      accentCustomHex: normalized,
      theme: get().theme,
    })
  },
  setNotificationsEnabled: (enabled) => {
    set({ notificationsEnabled: enabled })
    void persistUiState({ notificationsEnabled: enabled })
  },
  setCompletionSoundEnabled: (enabled) => {
    set({ completionSoundEnabled: enabled })
    void persistUiState({ completionSoundEnabled: enabled })
  },
  setDebugLoggingEnabled: (enabled) => {
    syncDebugFlag(enabled)
    set({ debugLoggingEnabled: enabled })
    void persistUiState({ debugLoggingEnabled: enabled })
  },
  setCrashReportingEnabled: (enabled) => {
    syncCrashReportingFlag(enabled)
    set({ crashReportingEnabled: enabled })
    void persistUiState({ crashReportingEnabled: enabled })
  },
  setBootstrapped: (value) => set({ isBootstrapped: value }),
  pushRecentCwd: (cwd) => {
    const trimmed = cwd.trim()
    if (!trimmed) return
    set((state) => {
      const next = [
        trimmed,
        ...state.recentCwds.filter((p) => p !== trimmed),
      ].slice(0, 10)
      void persistUiState({ recentCwds: next })
      return { recentCwds: next }
    })
  },
  setRecentCwds: (cwds) => {
    const next = cwds.filter((p) => p.trim().length > 0).slice(0, 10)
    set({ recentCwds: next })
  },
  toggleSessionPinned: (id) =>
    set((state) => {
      const isPinned = state.pinnedSessionIds.includes(id)
      const pinnedSessionIds = isPinned
        ? state.pinnedSessionIds.filter((sid) => sid !== id)
        : [...state.pinnedSessionIds, id]
      const archivedSessionIds = isPinned
        ? state.archivedSessionIds
        : state.archivedSessionIds.filter((sid) => sid !== id)
      void persistUiState({ pinnedSessionIds, archivedSessionIds })
      return { pinnedSessionIds, archivedSessionIds }
    }),
  setSessionArchived: (id, archived) =>
    set((state) => {
      const archivedSessionIds = archived
        ? state.archivedSessionIds.includes(id)
          ? state.archivedSessionIds
          : [...state.archivedSessionIds, id]
        : state.archivedSessionIds.filter((sid) => sid !== id)
      const pinnedSessionIds = archived
        ? state.pinnedSessionIds.filter((sid) => sid !== id)
        : state.pinnedSessionIds
      void persistUiState({ pinnedSessionIds, archivedSessionIds })
      return { pinnedSessionIds, archivedSessionIds }
    }),
  setPinnedSessionIds: (ids) => set({ pinnedSessionIds: ids }),
  setSidebarProjectSort: (sort) => {
    set({ sidebarProjectSort: sort })
    void persistUiState({ sidebarProjectSort: sort })
  },
  setSidebarProjectVisibility: (visibility) => {
    set({ sidebarProjectVisibility: visibility })
    void persistUiState({ sidebarProjectVisibility: visibility })
  },
  setArchivedSessionIds: (ids) => set({ archivedSessionIds: ids }),
  openChatTab: (id) =>
    set((state) => {
      if (state.openChatSessionIds.includes(id)) return state
      const openChatSessionIds = [...state.openChatSessionIds, id].slice(-20)
      void persistUiState({ openChatSessionIds })
      return { openChatSessionIds }
    }),
  closeChatTab: (id) => {
    const state = get()
    const ids = state.openChatSessionIds
    const idx = ids.indexOf(id)
    if (idx < 0) {
      return state.activeSessionId === id ? null : state.activeSessionId
    }
    const openChatSessionIds = ids.filter((sid) => sid !== id)
    void persistUiState({ openChatSessionIds })
    set({ openChatSessionIds })
    if (state.activeSessionId !== id) return state.activeSessionId
    return openChatSessionIds[idx] ?? openChatSessionIds[idx - 1] ?? null
  },
  setOpenChatSessionIds: (ids) => {
    const openChatSessionIds = [...new Set(ids.filter(Boolean))].slice(-20)
    set({ openChatSessionIds })
  },
  markUnread: (sessionId) =>
    set((state) => ({
      unreadBySession: {
        ...state.unreadBySession,
        [sessionId]: (state.unreadBySession[sessionId] ?? 0) + 1,
      },
    })),
  pushToast: (text, kind, action) => {
    toastCounter += 1
    const id = `toast-${toastCounter}`
    const sonnerFn = kind === "success" ? toast.success : toast.error
    sonnerFn(text, {
      id,
      ...(action
        ? { action: { label: action.label, onClick: action.onAction } }
        : {}),
    })
    set((state) => ({ toasts: [...state.toasts, { id, text, kind, action }] }))
  },
  dismissToast: (id) => {
    toast.dismiss(id)
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }))
  },
  setActiveTheme: (id) => {
    const { customThemes, theme } = get()
    set({ activeThemeId: id })
    void persistUiState({ activeThemeId: id })
    syncCustomThemeToDom(id, customThemes, theme)
  },
  upsertCustomTheme: (spec) => {
    set((state) => {
      const next = state.customThemes.some((t) => t.id === spec.id)
        ? state.customThemes.map((t) => (t.id === spec.id ? spec : t))
        : [...state.customThemes, spec]
      void persistUiState({ customThemes: next })
      return { customThemes: next }
    })
    const { activeThemeId, theme } = get()
    if (activeThemeId === spec.id) {
      syncCustomThemeToDom(spec.id, get().customThemes, theme)
    }
  },
  deleteCustomTheme: (id) => {
    set((state) => {
      const customThemes = state.customThemes.filter((t) => t.id !== id)
      const activeThemeId =
        state.activeThemeId === id ? FACTORY_THEME_ID : state.activeThemeId
      void persistUiState({ customThemes, activeThemeId })
      if (activeThemeId === FACTORY_THEME_ID) {
        clearThemeTokensFromDom()
      }
      return { customThemes, activeThemeId }
    })
  },
  importThemeJson: (raw) => {
    const result = parseThemeJson(raw)
    if (!result.ok) return result
    get().upsertCustomTheme(result.spec)
    get().setActiveTheme(result.spec.id)
    return result
  },
})
