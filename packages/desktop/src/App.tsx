import { useEffect, useRef, useState } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useGlobalSessionEvents } from "./hooks/useGlobalSessionEvents"
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts"
import { useSessions } from "./hooks/useSessions"
import { useViewportWidth } from "./hooks/useViewportWidth"
import { isBrowserPreview } from "./lib/browserMock"
import {
  cancel,
  isConfigured,
  listSessions,
  resumeSession,
} from "./lib/tauri"
import {
  CommandPalette,
  RightPanel,
  SearchModal,
  SessionSidebar,
} from "./components/organisms"
import { ToastHost } from "./components/molecules"
import { ChatPage } from "./pages/ChatPage"
import { SettingsPage } from "./pages/SettingsPage"
import { WelcomePage } from "./pages/WelcomePage"
import { restoreUiState, useAppStore } from "./stores/appStore"
import { cn } from "./lib/utils"

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      refetchOnWindowFocus: false,
    },
  },
})

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
      void cancel(sessionId)
      return true
    },
  }

  useKeyboardShortcuts(handlersRef)

  useEffect(() => {
    const bootstrap = async () => {
      try {
        const [configured, ui] = await Promise.all([
          isConfigured(),
          restoreUiState(),
        ])

        setTheme(ui.theme === "light" ? "light" : "dark")

        if (!configured) {
          setRoute("welcome")
          return
        }

        if (ui.activeSessionId) {
          try {
            await resumeSession(ui.activeSessionId)
            useAppStore.getState().setActiveSessionId(ui.activeSessionId)
          } catch {
            useAppStore.getState().setActiveSessionId(null)
          }
        } else if (isBrowserPreview()) {
          try {
            const sessions = await listSessions()
            const first = sessions[0]
            if (first) {
              await resumeSession(first.id)
              useAppStore.getState().setActiveSessionId(first.id)
            }
          } catch {
            // Preview can still open without a session
          }
        }

        if (ui.selectedModelId) {
          useAppStore.getState().setSelectedModelId(ui.selectedModelId)
        } else if (isBrowserPreview()) {
          useAppStore.getState().setSelectedModelId("anthropic/claude-sonnet-4")
        }

        if (ui.selectedIsolation) {
          useAppStore.getState().setSelectedIsolation(ui.selectedIsolation)
        }

        // Effort moved from a single global setting to per-model (reference
        // design: effort is picked FOR a specific model, in its dropdown row).
        // Migration: if a legacy `selectedEffort` exists and we haven't
        // captured any per-model efforts yet, apply it once to the current
        // model and drop the legacy value so this only runs the first launch
        // after upgrading.
        if (ui.effortByModel) {
          for (const [modelId, effort] of Object.entries(ui.effortByModel)) {
            useAppStore.getState().setEffortForModel(modelId, effort)
          }
        } else if (ui.selectedEffort) {
          const currentModel = useAppStore.getState().selectedModelId
          if (currentModel) {
            useAppStore.getState().setEffortForModel(currentModel, ui.selectedEffort)
          }
          useAppStore.getState().setSelectedEffort(null)
        }

        if (ui.composerMode) {
          useAppStore.getState().setComposerMode(ui.composerMode)
        }

        if (ui.defaultPermissionMode) {
          useAppStore.getState().setDefaultPermissionMode(ui.defaultPermissionMode)
        }

        if (typeof ui.notificationsEnabled === "boolean") {
          useAppStore.getState().setNotificationsEnabled(ui.notificationsEnabled)
        }
        if (typeof ui.completionSoundEnabled === "boolean") {
          useAppStore.getState().setCompletionSoundEnabled(ui.completionSoundEnabled)
        }

        if (ui.recentCwds?.length) {
          useAppStore.getState().setRecentCwds(ui.recentCwds)
        }

        if (ui.pinnedSessionIds) {
          useAppStore.getState().setPinnedSessionIds(ui.pinnedSessionIds)
        }
        if (ui.archivedSessionIds) {
          useAppStore.getState().setArchivedSessionIds(ui.archivedSessionIds)
        }

        if (ui.sidebarCollapsed) {
          useAppStore.getState().setSidebarCollapsed(true)
        }

        if (ui.rightPanelOpen) {
          useAppStore.getState().setRightPanelOpen(true)
        }
        if (ui.rightPanelTab) {
          useAppStore.getState().setRightPanelTab(ui.rightPanelTab)
        }
        if (typeof ui.rightPanelWidth === "number") {
          useAppStore.getState().setRightPanelWidth(ui.rightPanelWidth)
        }
        if (typeof ui.sidebarWidth === "number") {
          useAppStore.getState().setSidebarWidth(ui.sidebarWidth)
        }

        setRoute("chat")
      } catch {
        setRoute("welcome")
      } finally {
        useAppStore.getState().setBootstrapped(true)
      }
    }

    void bootstrap()
  }, [setRoute, setTheme])

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
          route === "automations" ||
          route === "memory" ? (
            <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
              <SettingsPage embedded />
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
  return (
    <QueryClientProvider client={queryClient}>
      <div className="h-full">
        <AppRoutes />
      </div>
    </QueryClientProvider>
  )
}

export default App
