import type { StateCreator } from "zustand"
import type { AppState, ComposerSliceState } from "../types"
import { FLEX_MODE_ENABLED } from "../../lib/featureFlags"
import { persistUiState } from "../persist"

export const createComposerSlice: StateCreator<
  AppState,
  [],
  [],
  ComposerSliceState
> = (set, get) => ({
  draftsBySession: {},
  orphanDraft: "",
  composerMode: "agent",
  defaultPermissionMode: "default",
  sessionBypassBySession: {},
  selectedModelId: null,
  selectedIsolation: null,
  selectedReuseWorkspaceId: null,
  selectedEffort: null,
  effortByModel: {},
  attachments: [],
  getComposerDraft: () => {
    const state = get()
    if (!state.activeSessionId) return state.orphanDraft
    return state.draftsBySession[state.activeSessionId] ?? ""
  },
  setComposerDraft: (draft, forSessionId) => {
    const sessionId = forSessionId ?? get().activeSessionId
    if (!sessionId) {
      set({ orphanDraft: draft })
      return
    }
    set((state) => ({
      draftsBySession: { ...state.draftsBySession, [sessionId]: draft },
    }))
  },
  setComposerMode: (mode) => {
    const next = mode === "flex" && !FLEX_MODE_ENABLED ? "agent" : mode
    set({ composerMode: next })
    void persistUiState({ composerMode: next })
  },
  setDefaultPermissionMode: (mode) => {
    set({ defaultPermissionMode: mode })
    void persistUiState({ defaultPermissionMode: mode })
  },
  setSessionBypass: (sessionId, enabled) =>
    set((state) => {
      const next = { ...state.sessionBypassBySession }
      if (enabled) {
        next[sessionId] = true
      } else {
        delete next[sessionId]
      }
      return { sessionBypassBySession: next }
    }),
  setSelectedModelId: (id) => {
    set({ selectedModelId: id })
    void persistUiState({ selectedModelId: id })
  },
  setSelectedIsolation: (isolation) => {
    set({ selectedIsolation: isolation })
    void persistUiState({ selectedIsolation: isolation })
  },
  setSelectedReuseWorkspaceId: (id) => {
    // Never persisted to disk: a workspace id is only meaningful for the
    // very next `create_session` this draft becomes. Persisting it across
    // restarts would carry a stale reference into the next launch.
    set({ selectedReuseWorkspaceId: id })
  },
  setSelectedEffort: (effort) => {
    set({ selectedEffort: effort })
    void persistUiState({ selectedEffort: effort })
  },
  setEffortForModel: (modelId, effort) =>
    set((state) => {
      const next = { ...state.effortByModel }
      if (effort === null) {
        delete next[modelId]
      } else {
        next[modelId] = effort
      }
      void persistUiState({ effortByModel: next })
      return { effortByModel: next }
    }),
  getEffortForModel: (modelId) => {
    if (!modelId) return null
    return get().effortByModel[modelId] ?? null
  },
  addAttachment: (att) =>
    set((state) => ({ attachments: [...state.attachments, att] })),
  removeAttachment: (id) =>
    set((state) => ({
      attachments: state.attachments.filter((a) => a.id !== id),
    })),
  clearAttachments: () => set({ attachments: [] }),
})
