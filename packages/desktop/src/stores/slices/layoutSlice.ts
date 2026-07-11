import type { StateCreator } from "zustand"
import type { AppState, LayoutSliceState, RightPanelTab, Viewport } from "../types"
import {
  SIDEBAR_DEFAULT_WIDTH,
  RIGHT_PANEL_DEFAULT_WIDTH,
  clampRightPanelWidth,
  clampSidebarWidth,
} from "../layoutConstants"
import { persistUiState } from "../persist"

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
  rightPanelTab: "plan" as RightPanelTab,
  rightPanelWidth: RIGHT_PANEL_DEFAULT_WIDTH,
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
    // Mobile (narrow/tight): only one full-width overlay at a time — opening
    // the sidebar closes the right panel.
    if (state.viewport !== "wide" && !collapsed && state.rightPanelOpen) {
      set({ sidebarCollapsed: collapsed, rightPanelOpen: false })
      void persistUiState({ sidebarCollapsed: collapsed, rightPanelOpen: false })
      return
    }
    set({ sidebarCollapsed: collapsed })
    void persistUiState({ sidebarCollapsed: collapsed })
  },
  toggleSidebarCollapsed: () => {
    get().setSidebarCollapsed(!get().sidebarCollapsed)
  },
  setSidebarWidth: (width, persist = true) => {
    const state = get()
    // Only the wide, side-by-side layout needs the cross-pane clamp — at
    // narrow/tight the right panel is a full-width overlay, not sharing row
    // space with the sidebar, so it must not shrink the sidebar's ceiling.
    const rightPanelVisible = state.viewport === "wide" && state.rightPanelOpen
    const clamped = clampSidebarWidth(width, state.rightPanelWidth, rightPanelVisible)
    set({ sidebarWidth: clamped })
    if (persist) void persistUiState({ sidebarWidth: clamped })
  },
  setRightPanelOpen: (open) => {
    const state = get()
    // Mobile (narrow/tight): only one full-width overlay at a time — opening
    // the right panel collapses the left sidebar.
    if (state.viewport !== "wide" && open && !state.sidebarCollapsed) {
      set({ rightPanelOpen: open, sidebarCollapsed: true })
      void persistUiState({ rightPanelOpen: open, sidebarCollapsed: true })
      return
    }
    set({ rightPanelOpen: open })
    void persistUiState({ rightPanelOpen: open })
  },
  toggleRightPanel: () => {
    get().setRightPanelOpen(!get().rightPanelOpen)
  },
  setRightPanelTab: (tab) => {
    set({ rightPanelTab: tab })
    void persistUiState({ rightPanelTab: tab })
  },
  setRightPanelWidth: (width, persist = true) => {
    const state = get()
    // Only the wide, side-by-side layout needs the cross-pane clamp — at
    // narrow/tight the sidebar is a full-width overlay, not sharing row
    // space with the right panel, so it must not shrink the panel's ceiling.
    const sidebarVisible = state.viewport === "wide" && !state.sidebarCollapsed
    const clamped = clampRightPanelWidth(width, state.sidebarWidth, sidebarVisible)
    set({ rightPanelWidth: clamped })
    if (persist) void persistUiState({ rightPanelWidth: clamped })
  },
  setViewport: (viewport) => {
    const state = get()
    if (state.viewport === viewport) {
      // Same classification, but the window may still have shrunk within it
      // (e.g. 1280 -> 1000, both "wide") — re-clamp so a previously-valid
      // side-by-side layout can't crush the chat column below CHAT_MIN_WIDTH.
      // Re-entrant through the setters themselves (not the raw clamp helper)
      // so persistence/state stay in sync exactly like a live sash drag.
      if (viewport === "wide") {
        get().setSidebarWidth(state.sidebarWidth)
        get().setRightPanelWidth(state.rightPanelWidth)
      }
      return
    }

    const wasNarrow = state.viewport !== "wide"
    const isNarrow = viewport !== "wide"

    if (!wasNarrow && isNarrow) {
      // Entering narrow/tight: remember the user's own preferences for both
      // the sidebar and the right panel, then force-collapse/close both —
      // mobile only ever shows one full-width overlay at a time, never a
      // side-by-side layout (auto-collapse/close must not clobber the
      // preferences so they can be restored below).
      set({
        sidebarCollapsedBeforeNarrow: state.sidebarCollapsed,
        sidebarCollapsed: true,
        rightPanelOpenBeforeNarrow: state.rightPanelOpen,
        rightPanelOpen: false,
        viewport,
      })
      return
    }

    if (wasNarrow && !isNarrow) {
      // Back to wide: restore whatever the user had before narrowing.
      const restoreSidebar =
        state.sidebarCollapsedBeforeNarrow ?? state.sidebarCollapsed
      const restoreRightPanel =
        state.rightPanelOpenBeforeNarrow ?? state.rightPanelOpen
      set({
        sidebarCollapsed: restoreSidebar,
        sidebarCollapsedBeforeNarrow: null,
        rightPanelOpen: restoreRightPanel,
        rightPanelOpenBeforeNarrow: null,
        viewport,
      })
      if (restoreSidebar !== state.sidebarCollapsed) {
        void persistUiState({ sidebarCollapsed: restoreSidebar })
      }
      if (restoreRightPanel !== state.rightPanelOpen) {
        void persistUiState({ rightPanelOpen: restoreRightPanel })
      }
      // Re-clamp now that both panes may be visible side-by-side again at
      // "wide" — the persisted widths could have been set while narrow (no
      // cross-pane constraint applied) or the window could have shrunk while
      // narrow/tight (whose overlay widths aren't clamped against chat).
      get().setSidebarWidth(get().sidebarWidth)
      get().setRightPanelWidth(get().rightPanelWidth)
      return
    }

    // narrow <-> tight: same auto-collapsed/closed behavior, just update the label.
    set({ viewport })
  },
})
