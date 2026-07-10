import { useEffect, useRef } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useGlobalSessionEvents } from "./hooks/useGlobalSessionEvents"
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts"
import { useSessions } from "./hooks/useSessions"
import { isBrowserPreview } from "./lib/browserMock"
import {
  cancel,
  isConfigured,
  listSessions,
  resumeSession,
} from "./lib/tauri"
import { RightPanel, SessionSidebar } from "./components/organisms"
import { AutomationsPage } from "./pages/AutomationsPage"
import { ChatPage } from "./pages/ChatPage"
import { CustomizePage } from "./pages/CustomizePage"
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
  const toggleSidebarSearch = useAppStore((s) => s.toggleSidebarSearch)
  const toggleSidebarCollapsed = useAppStore((s) => s.toggleSidebarCollapsed)
  const toggleRightPanel = useAppStore((s) => s.toggleRightPanel)
  const setTheme = useAppStore((s) => s.setTheme)
  const { newAgent } = useSessions()
  useGlobalSessionEvents()

  // Stable handlers — avoid rebinding the window listener every render.
  const handlersRef = useRef({
    onSend: () => {},
    onNewSession: () => {},
    onSearch: () => {},
    onFocusComposer: () => {},
    onCancel: (): boolean => false,
    onToggleSidebar: () => {},
    onToggleRightPanel: () => {},
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
      setRoute("chat")
      toggleSidebarSearch()
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
    onCancel: () => {
      const state = useAppStore.getState()
      if (state.sidebarSearchOpen) {
        state.setSidebarSearchOpen(false)
        return true
      }
      if (!state.isStreaming || !state.activeSessionId) return false
      const sessionId = state.activeSessionId
      state.setIsStreaming(false)
      state.setSessionStreaming(sessionId, false)
      state.clearStreamingForSession(sessionId)
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

        if (ui.composerMode) {
          useAppStore.getState().setComposerMode(ui.composerMode)
        }

        if (ui.recentCwds?.length) {
          useAppStore.getState().setRecentCwds(ui.recentCwds)
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
  // settings round-trips (Cursor Glass: content swap, not full remount).
  return (
    <div className="flex h-full bg-bg">
      <SessionSidebar />
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
        {route === "settings" ? (
          <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
            <SettingsPage embedded />
          </div>
        ) : null}
        {route === "customize" ? (
          <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
            <CustomizePage embedded />
          </div>
        ) : null}
        {route === "automations" ? (
          <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
            <AutomationsPage embedded />
          </div>
        ) : null}
      </div>
      <RightPanel />
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
