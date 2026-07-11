import type { StateCreator } from "zustand"
import type { AppState, UiSliceState, UiTheme } from "../types"
import { persistUiState } from "../persist"

const applyThemeToDom = (theme: UiTheme) => {
  if (typeof document === "undefined") return
  document.documentElement.setAttribute("data-theme", theme)
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
  notificationsEnabled: true,
  completionSoundEnabled: false,
  isBootstrapped: false,
  recentCwds: [],
  pinnedSessionIds: [],
  archivedSessionIds: [],
  unreadBySession: {},
  messageFeedback: {},
  toasts: [],
  setRoute: (route) =>
    set((state) => ({
      route,
      // Navigating away from chat leaves the panel with nothing sensible to
      // anchor to — close it rather than let it linger off-screen.
      subagentViewer: route === "chat" ? state.subagentViewer : null,
      // Legacy dedicated routes (settings/customize/automations/memory) now
      // all mount the same SettingsShell — preselect the nav section that
      // corresponds to whichever shortcut was clicked, so e.g. the sidebar's
      // "Memory" button still lands the user on the Memory section.
      settingsSection:
        route === "memory"
          ? "memory"
          : route === "automations"
            ? "automations"
            : route === "customize"
              ? "tools-mcp"
              : route === "settings"
                ? state.settingsSection
                : state.settingsSection,
    })),
  setSettingsSection: (section) => set({ settingsSection: section }),
  setTheme: (theme) => {
    applyThemeToDom(theme)
    set({ theme })
    void persistUiState({ theme })
  },
  toggleTheme: () => {
    const next = get().theme === "dark" ? "light" : "dark"
    get().setTheme(next)
  },
  setNotificationsEnabled: (enabled) => {
    set({ notificationsEnabled: enabled })
    void persistUiState({ notificationsEnabled: enabled })
  },
  setCompletionSoundEnabled: (enabled) => {
    set({ completionSoundEnabled: enabled })
    void persistUiState({ completionSoundEnabled: enabled })
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
  markUnread: (sessionId) =>
    set((state) => ({
      unreadBySession: {
        ...state.unreadBySession,
        [sessionId]: (state.unreadBySession[sessionId] ?? 0) + 1,
      },
    })),
  setMessageFeedback: (messageId, value) =>
    set((state) => {
      const next = { ...state.messageFeedback }
      if (value === null) {
        delete next[messageId]
      } else {
        next[messageId] = value
      }
      return { messageFeedback: next }
    }),
  pushToast: (text, kind, action) => {
    toastCounter += 1
    const id = `toast-${toastCounter}`
    set((state) => ({ toasts: [...state.toasts, { id, text, kind, action }] }))
  },
  dismissToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
})
