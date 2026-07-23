import type { StateCreator } from "zustand"
import type { AppState, LayoutSliceState, RightPanelTab, Viewport } from "../types"
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MIN_WIDTH,
  RIGHT_PANEL_DEFAULT_WIDTH,
  clampRightPanelWidth,
  clampSidebarWidth,
} from "../layoutConstants"
import { persistUiState } from "../persist"
import { isRightPanelTabEnabled } from "../../lib/featureFlags"

export const createLayoutSlice: StateCreator<
  AppState,
  [],
  [],
  LayoutSliceState
> = (set, get) => ({
  sidebarSearchOpen: false,
  sidebarSearchQuery: "",
  sidebarCollapsed: false,
  sidebarWidth: SIDEBAR_DEFAULT_WIDTH,
  rightPanelOpen: false,
  rightPanelTab: "changes" as RightPanelTab,
  rightPanelWidth: RIGHT_PANEL_DEFAULT_WIDTH,
  rightPanelCollapsed: false,
  rightPanelDragging: false,
  viewport: "wide" as Viewport,
  sidebarCollapsedBeforeNarrow: null,
  rightPanelOpenBeforeNarrow: null,
  setSidebarSearchOpen: (open) =>
    set((state) => ({
      sidebarSearchOpen: open,
      sidebarSearchQuery: open ? state.sidebarSearchQuery : "",
    })),
  setSidebarSearchQuery: (query) => set({ sidebarSearchQuery: query }),
  toggleSidebarSearch: () =>
    set((state) => ({
      sidebarSearchOpen: !state.sidebarSearchOpen,
      sidebarSearchQuery: state.sidebarSearchOpen ? "" : state.sidebarSearchQuery,
    })),
  setSidebarCollapsed: (collapsed) => {
    const state = get()
    const widen =
      !collapsed &&
      state.viewport === "wide" &&
      state.sidebarWidth <= SIDEBAR_MIN_WIDTH
    const nextWidth = widen ? SIDEBAR_DEFAULT_WIDTH : state.sidebarWidth
    if (state.viewport !== "wide" && !collapsed && state.contentLayout.mode === "split") {
      get().collapseSplit()
    }
    set({
      sidebarCollapsed: collapsed,
      ...(widen ? { sidebarWidth: nextWidth } : {}),
    })
    void persistUiState({
      sidebarCollapsed: collapsed,
      ...(widen ? { sidebarWidth: nextWidth } : {}),
    })
  },
  toggleSidebarCollapsed: () => {
    get().setSidebarCollapsed(!get().sidebarCollapsed)
  },
  setSidebarWidth: (width, persist = true) => {
    const state = get()
    const rightPanelVisible =
      state.viewport === "wide" && state.contentLayout.mode === "split"
    const clamped = clampSidebarWidth(width, state.rightPanelWidth, rightPanelVisible)
    set({ sidebarWidth: clamped })
    if (persist) void persistUiState({ sidebarWidth: clamped })
  },
  setRightPanelOpen: (open) => {
    if (open) {
      get().setRightPanelCollapsed(false)
      if (get().viewport !== "wide") {
        set({ rightPanelOpen: true, sidebarCollapsed: true })
        return
      }
      const sessionId = get().activeSessionId
      if (sessionId) {
        get().ensureDefaultWorkPane(sessionId)
      } else {
        get().ensureSplit()
      }
    } else {
      get().setRightPanelCollapsed(true)
      get().collapseSplit()
    }
  },
  toggleRightPanel: () => {
    get().toggleSplit()
  },
  setRightPanelTab: (tab) => {
    if (!isRightPanelTabEnabled(tab)) return
    const sessionId = get().activeSessionId
    if (!sessionId) {
      set({ rightPanelTab: tab })
      return
    }
    get().openToolBesideChat(sessionId, tab)
  },
  setRightPanelWidth: (width, persist = true) => {
    const state = get()
    const sidebarVisible = state.viewport === "wide" && !state.sidebarCollapsed
    const clamped = clampRightPanelWidth(width, state.sidebarWidth, sidebarVisible)
    set({ rightPanelWidth: clamped })
    if (persist) void persistUiState({ rightPanelWidth: clamped })
  },
  setRightPanelCollapsed: (collapsed) => {
    set({ rightPanelCollapsed: collapsed })
    void persistUiState({ rightPanelCollapsed: collapsed })
  },
  toggleRightPanelCollapsed: () => {
    get().setRightPanelCollapsed(!get().rightPanelCollapsed)
  },
  setRightPanelDragging: (dragging) => set({ rightPanelDragging: dragging }),
  setViewport: (viewport) => {
    const state = get()
    if (state.viewport === viewport) {
      if (viewport === "wide") {
        get().setSidebarWidth(state.sidebarWidth)
        get().setRightPanelWidth(state.rightPanelWidth)
      }
      return
    }

    const wasNarrow = state.viewport !== "wide"
    const isNarrow = viewport !== "wide"

    if (!wasNarrow && isNarrow) {
      const wasSplit = state.contentLayout.mode === "split"
      if (wasSplit) get().collapseSplit()
      set({
        sidebarCollapsedBeforeNarrow: state.sidebarCollapsed,
        sidebarCollapsed: true,
        rightPanelOpenBeforeNarrow: wasSplit,
        rightPanelOpen: false,
        viewport,
      })
      return
    }

    if (wasNarrow && !isNarrow) {
      const restoreSidebar =
        state.sidebarCollapsedBeforeNarrow ?? state.sidebarCollapsed
      const restoreSplit = state.rightPanelOpenBeforeNarrow ?? false
      set({
        sidebarCollapsed: restoreSidebar,
        sidebarCollapsedBeforeNarrow: null,
        rightPanelOpenBeforeNarrow: null,
        viewport,
      })
      if (restoreSidebar !== state.sidebarCollapsed) {
        void persistUiState({ sidebarCollapsed: restoreSidebar })
      }
      if (restoreSplit) get().ensureSplit()
      get().setSidebarWidth(get().sidebarWidth)
      get().setRightPanelWidth(get().rightPanelWidth)
      return
    }

    set({ viewport })
  },
})
