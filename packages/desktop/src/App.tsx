import { lazy, Suspense, useRef, useState } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useBootstrap } from "./hooks/useBootstrap"
import { useGlobalSessionEvents } from "./hooks/useGlobalSessionEvents"
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts"
import { useSessions } from "./hooks/useSessions"
import { useUpdaterCheck } from "./hooks/useUpdaterCheck"
import { useViewportWidth } from "./hooks/useViewportWidth"
import { isBrowserPreview } from "./lib/browserPreview"
import { AUTOMATIONS_UI_ENABLED } from "./lib/featureFlags"
import { cancel } from "./lib/tauri"
import {
  CommandPalette,
  RightPanel,
  SearchModal,
  SessionSidebar,
} from "./components/organisms"
import { ToastHost } from "./components/molecules"
import { ChatPage } from "./pages/ChatPage"
import { WelcomePage } from "./pages/WelcomePage"
import { useAppStore } from "./stores/appStore"
import { cn } from "./lib/utils"

const SettingsPage = lazy(() =>
  import("./pages/SettingsPage").then((m) => ({ default: m.SettingsPage })),
)

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
    },
  },
})

const NativeAppRequired = () => (
  <div className="flex h-full min-h-screen flex-col items-center justify-center gap-3 bg-bg px-6">
    <p className="text-[18px] font-medium text-ink">Desktop app required</p>
    <p className="max-w-[420px] text-center text-sm text-ink-muted">
      Browser preview has no backend. Run{" "}
      <code className="rounded bg-fill-3 px-1.5 py-0.5 text-[12px] text-ink">
        pnpm tauri dev
      </code>{" "}
      or open the installed app.
    </p>
  </div>
)

const AppRoutes = () => {
  const route = useAppStore((s) => s.route)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const setRoute = useAppStore((s) => s.setRoute)
  const toggleSidebarCollapsed = useAppStore((s) => s.toggleSidebarCollapsed)
  const toggleRightPanel = useAppStore((s) => s.toggleRightPanel)
  const setTheme = useAppStore((s) => s.setTheme)
  const { newAgent } = useSessions()
  useGlobalSessionEvents()
  // Gated on isBootstrapped so the first classification runs after
  // restoreUiState applies the persisted sidebarCollapsed value — otherwise
  // an early narrow classification could force-collapse before the user's
  // saved preference is even loaded (BEHAVIOR SPEC #5).
  useViewportWidth(isBootstrapped)
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)
  const [searchModalOpen, setSearchModalOpen] = useState(false)

  // Stable handlers — avoid rebinding the window listener every render.
  const handlersRef = useRef({
    onSend: () => {},
    onNewSession: () => {},
    onSearch: () => {},
    onFocusComposer: () => {},
    onCancel: (): boolean => false,
    onToggleSidebar: () => {},
    onToggleRightPanel: () => {},
    onToggleCommandPalette: () => {},
  })
  handlersRef.current = {
    onSend: () => {
      const composer = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      if (!composer) return
      composer.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", metaKey: true, bubbles: true }),
      )
    },
    onNewSession: () => {
      void newAgent()
    },
    onSearch: () => {
      if (useAppStore.getState().route === "welcome") return
      setSearchModalOpen((v) => !v)
    },
    onFocusComposer: () => {
      document.querySelector<HTMLTextAreaElement>("[data-composer]")?.focus()
    },
    onToggleSidebar: () => {
      toggleSidebarCollapsed()
    },
    onToggleRightPanel: () => {
      toggleRightPanel()
    },
    onToggleCommandPalette: () => {
      setCommandPaletteOpen((v) => !v)
    },
    onCancel: () => {
      const state = useAppStore.getState()
      if (commandPaletteOpen) {
        setCommandPaletteOpen(false)
        return true
      }
      if (searchModalOpen) {
        setSearchModalOpen(false)
        return true
      }
      // Narrow/tight: an open sidebar or right-panel overlay covers the
      // chat area — Esc closes it before falling through to turn-cancel.
      if (state.viewport !== "wide") {
        if (!state.sidebarCollapsed) {
          state.setSidebarCollapsed(true)
          return true
        }
        if (state.rightPanelOpen) {
          state.setRightPanelOpen(false)
          return true
        }
      }
      if (!state.isStreaming || !state.activeSessionId) return false
      const sessionId = state.activeSessionId
      state.setIsStreaming(false)
      state.setSessionStreaming(sessionId, false)
      state.clearStreamingForSession(sessionId)
      // Force-close any rows still marked running (spinner backstop) — the
      // engine may never emit a matching turn_completed/session_error, e.g.
      // if the process already died. See useSessionEvents' sweepRequests.
      state.requestSweep(sessionId)
      // Keep this session's event subscription alive until the cancelled
      // turn's terminal event is actually observed (see
      // useGlobalSessionEvents.ts) — the engine cancel is async, and
      // streamingSessions was just cleared above.
      state.setSessionDraining(sessionId, true)
      void cancel(sessionId)
      return true
    },
  }

  useKeyboardShortcuts(handlersRef)

  useBootstrap(setRoute, setTheme)
  useUpdaterCheck(isBootstrapped && route !== "welcome")

  if (!isBootstrapped) {
    return (
      <div className="flex h-full items-center justify-center bg-bg text-sm text-ink-muted">
        Loading…
      </div>
    )
  }

  if (route === "welcome") return <WelcomePage />

  // Persistent sidebar + keep Chat mounted so timeline/subscriptions survive
  // settings round-trips (reference glass: content swap, not full remount).
  return (
    <div className="relative flex h-full bg-bg">
      {/* Root is `relative` so SessionSidebar's mobile overlay (absolute,
       * anchored to this container's left edge — see SessionSidebar.tsx's
       * `narrow` handling) spans the full app width, not just the chat area. */}
      <SessionSidebar onOpenSearch={() => setSearchModalOpen(true)} />
      {/* Relative + flex-row: at "wide" the right panel lays out as a normal
       * side-by-side column (unchanged from before); at "narrow"/"tight" it
       * switches to an absolute overlay anchored to this container's right
       * edge, so the overlay + backdrop cover only the chat area, not the
       * sidebar (see RightPanel.tsx's `narrow` handling). */}
      <div className="relative flex min-h-0 min-w-0 flex-1">
        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
          <div
            className={cn(
              "flex h-full min-h-0 flex-1 flex-col",
              "transition-opacity duration-[var(--duration-normal)] ease-[var(--easing-default)]",
              route !== "chat" && "pointer-events-none opacity-0",
            )}
            aria-hidden={route !== "chat"}
          >
            <ChatPage embedded />
          </div>
          {route === "settings" ||
          route === "customize" ||
          (AUTOMATIONS_UI_ENABLED && route === "automations") ||
          route === "memory" ? (
            <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
              <Suspense
                fallback={
                  <div className="flex h-full items-center justify-center text-sm text-ink-muted">
                    Loading…
                  </div>
                }
              >
                <SettingsPage embedded />
              </Suspense>
            </div>
          ) : null}
        </div>
        <RightPanel />
      </div>
      <CommandPalette
        open={commandPaletteOpen}
        onClose={() => setCommandPaletteOpen(false)}
      />
      <SearchModal
        open={searchModalOpen}
        onClose={() => setSearchModalOpen(false)}
      />
      <ToastHost />
    </div>
  )
}

const App = () => {
  if (isBrowserPreview()) return <NativeAppRequired />
  return (
    <QueryClientProvider client={queryClient}>
      <div className="h-full">
        <AppRoutes />
      </div>
    </QueryClientProvider>
  )
}

export default App
