import type { StateCreator } from "zustand"
import { toast } from "sonner"
import type { AppState, UiSliceState, UiTheme } from "../types"
import type { AccentId } from "../../lib/accent"
import {
  applyAccentToDom,
  DEFAULT_ACCENT_ID,
  DEFAULT_CUSTOM_ACCENT,
  normalizeAccentHex,
} from "../../lib/accent"
import { AUTOMATIONS_UI_ENABLED } from "../../lib/featureFlags"
import { persistUiState } from "../persist"
import { syncCrashReportingFlag, syncDebugFlag } from "../../lib/debug/log"

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
  openChatSessionIds: [],
  unreadBySession: {},
  toasts: [],
  setRoute: (route) =>
    set((state) => {
      // Automations UI is feature-flagged off by default — treat the legacy
      // route as Settings so stale shortcuts / deep-links don't blank out.
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
        // Navigating away from chat leaves the panel with nothing sensible to
        // anchor to — close it rather than let it linger off-screen.
        subagentViewer: effectiveRoute === "chat" ? state.subagentViewer : null,
        // Legacy dedicated routes (settings/customize/automations/memory) now
        // all mount the same SettingsShell — preselect the nav section that
        // corresponds to whichever shortcut was clicked, so e.g. the sidebar's
        // "Memory" button still lands the user on the Memory section.
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
    const { accentId, accentCustomHex } = get()
    syncAccentToDom({ accentId, accentCustomHex, theme })
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
      // Pinning unarchives (mutually exclusive with archive).
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
      // Archiving unpins (mutually exclusive with pin).
      const pinnedSessionIds = archived
        ? state.pinnedSessionIds.filter((sid) => sid !== id)
        : state.pinnedSessionIds
      void persistUiState({ pinnedSessionIds, archivedSessionIds })
      return { pinnedSessionIds, archivedSessionIds }
    }),
  setPinnedSessionIds: (ids) => set({ pinnedSessionIds: ids }),
  setArchivedSessionIds: (ids) => set({ archivedSessionIds: ids }),
  openChatTab: (id) =>
    set((state) => {
      if (state.openChatSessionIds.includes(id)) return state
      // Cap so a long day of switching does not grow an endless strip.
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
    // Prefer the tab to the right, else the one to the left.
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
    const opts = action
      ? {
          action: {
            label: action.label,
            onClick: () => action.onAction(),
          },
        }
      : undefined
    if (kind === "success") toast.success(text, opts)
    else toast.error(text, opts)
  },
  dismissToast: (id) => {
    toast.dismiss(id)
  },
})
