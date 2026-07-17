import { create } from "zustand"
import type { AppState } from "./types"
import { emptyStreaming } from "./types"
import { createSessionSlice } from "./slices/sessionSlice"
import { createComposerSlice } from "./slices/composerSlice"
import { createLayoutSlice } from "./slices/layoutSlice"
import { createContentLayoutSlice } from "./slices/contentLayoutSlice"
import { createUiSlice } from "./slices/uiSlice"
import { createPanelExtrasSlice } from "./slices/panelExtrasSlice"

export { CHAT_MIN_WIDTH } from "./layoutConstants"
export type {
  UiTheme,
  Viewport,
  RightPanelTab,
  TerminalMeta,
  BrowserViewportPreset,
  BrowserSessionState,
  AppState,
  ContentLayout,
  ContentTab,
} from "./types"
export { sessionScopeKey, sessionHasActivity } from "./types"
export { persistUiState, flushPersistUiState, restoreUiState } from "./persist"
export type { UiPersisted } from "./persist"

export const useAppStore = create<AppState>()((...a) => ({
  ...createSessionSlice(...a),
  ...createComposerSlice(...a),
  ...createLayoutSlice(...a),
  ...createContentLayoutSlice(...a),
  ...createUiSlice(...a),
  ...createPanelExtrasSlice(...a),
}))

export const emptyStreamingBuffers = emptyStreaming