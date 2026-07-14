import type { StateCreator } from "zustand"
import type { AppState, LayoutSliceState, RightPanelTab, Viewport } from "../types"
import { sessionScopeKey } from "../types"
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
  rightPanelTab: "plan" as RightPanelTab,
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
    // When re-expanding in wide mode, bump a stuck-at-min width back to the
    // default so the sidebar actually opens to a usable size (persisted min
    // from a prior clamp can leave it "half open").
    const widen =
      !collapsed &&
      state.viewport === "wide" &&
      state.sidebarWidth <= SIDEBAR_MIN_WIDTH
    const nextWidth = widen ? SIDEBAR_DEFAULT_WIDTH : state.sidebarWidth
    // Mobile (narrow/tight): only one full-width overlay at a time — opening
    // the sidebar closes the right panel.
    if (state.viewport !== "wide" && !collapsed && state.rightPanelOpen) {
      set({
        sidebarCollapsed: collapsed,
        rightPanelOpen: false,
        ...(widen ? { sidebarWidth: nextWidth } : {}),
      })
      void persistUiState({
        sidebarCollapsed: collapsed,
        rightPanelOpen: false,
        ...(widen ? { sidebarWidth: nextWidth } : {}),
      })
      return
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
    // Opening from a fully-closed panel always expands — a persisted
    // `rightPanelCollapsed: true` must not leave the user staring at the
    // 40px « strip with no tab bar / "+" control after ⌘J or a header click.
    // Already-open → stay as-is (collapse is an explicit » action).
    const expandFromClosed = open && !state.rightPanelOpen
    // Mobile (narrow/tight): only one full-width overlay at a time — opening
    // the right panel collapses the left sidebar.
    if (state.viewport !== "wide" && open && !state.sidebarCollapsed) {
      set({
        rightPanelOpen: open,
        sidebarCollapsed: true,
        ...(expandFromClosed ? { rightPanelCollapsed: false } : {}),
      })
      void persistUiState({
        rightPanelOpen: open,
        sidebarCollapsed: true,
        ...(expandFromClosed ? { rightPanelCollapsed: false } : {}),
      })
      return
    }
    set({
      rightPanelOpen: open,
      ...(expandFromClosed ? { rightPanelCollapsed: false } : {}),
    })
    void persistUiState({
      rightPanelOpen: open,
      ...(expandFromClosed ? { rightPanelCollapsed: false } : {}),
    })
  },
  toggleRightPanel: () => {
    const state = get()
    // Open + collapsed: ⌘J / header button expands instead of closing —
    // otherwise the user cycles closed → collapsed strip forever and never
    // reaches the tab bar where "+" lives.
    if (state.rightPanelOpen && state.rightPanelCollapsed) {
      get().setRightPanelCollapsed(false)
      return
    }
    const opening = !state.rightPanelOpen
    get().setRightPanelOpen(opening)
    // ⌘J into an empty strip used to show only "+" until a plan arrived
    // (auto-reveal) or the user dug into the add menu. Seed the session's
    // preferred / default tab so Plan (and any remembered tab) opens empty.
    if (opening) {
      const sessionId = get().activeSessionId
      if (!sessionId) return
      const key = sessionScopeKey(sessionId)
      const openIds = (get().openTabsBySession[key] ?? []).filter(
        isRightPanelTabEnabled,
      )
      if (openIds.length > 0) return
      let preferred =
        get().selectedTabBySession[key] ?? get().rightPanelTab ?? "plan"
      if (!isRightPanelTabEnabled(preferred)) preferred = "plan"
      get().setRightPanelTab(preferred)
    }
  },
  setRightPanelTab: (tab) => {
    if (!isRightPanelTabEnabled(tab)) return
    set({ rightPanelTab: tab })
    void persistUiState({ rightPanelTab: tab })
    // Switching to a tab always counts as "opening" it for the active
    // session — every existing call site (plan-approval auto-reveal,
    // dev-server toast, Files-Changed card, ContextBar, CommandPalette,
    // terminal auto-activate on exec output) already pairs this with
    // setRightPanelOpen(true), so hooking in here is the single choke
    // point that makes them all register in the "Open Tabs" sidebar
    // section too, with no per-call-site changes needed.
    const sessionId = get().activeSessionId
    if (sessionId) {
      const key = sessionScopeKey(sessionId)
      get().openTab(key, tab)
      // Remember this session's own selection so switching away and back
      // (see `setActiveSessionId`) restores the same tab instead of
      // whatever the global `rightPanelTab` happened to be left at by a
      // DIFFERENT session (see BUG #37).
      set((state) => ({
        selectedTabBySession: { ...state.selectedTabBySession, [key]: tab },
      }))
    }
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
