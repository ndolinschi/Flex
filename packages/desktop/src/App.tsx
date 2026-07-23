import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { listen } from "@tauri-apps/api/event"
import { useBootstrap } from "./hooks/useBootstrap"
import { useGlobalSessionEvents } from "./hooks/useGlobalSessionEvents"
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts"
import { useSessions } from "./hooks/useSessions"
import { useUpdaterCheck } from "./hooks/useUpdaterCheck"
import { useViewportWidth } from "./hooks/useViewportWidth"
import { isBrowserPreview } from "./lib/browserPreview"
import { AUTOMATIONS_UI_ENABLED } from "./lib/featureFlags"
import { browserOpen, cancel, createSession } from "./lib/tauri"
import { newAgentCreateInput } from "./lib/sessions"
import {
  CommandPalette,
  SearchModal,
  SessionSidebar,
  WindowTitleBar,
} from "./components/organisms"
import { TitleBarChromeHost } from "./components/organisms/TitleBarChrome"
import { ContentWorkspace } from "./components/organisms/content/ContentWorkspace"
import { ToastHost } from "./components/molecules"
import { Spinner } from "./components/atoms"
import { WelcomePage } from "./pages/WelcomePage"
import {
  IdePlayground,
  isIdePlaygroundHash,
} from "./playground/ide-mock"
import { sessionScopeKey, useAppStore } from "./stores/appStore"
import { cn } from "./lib/utils"
import { startDesktopIdlePrefetch } from "./lib/idlePrefetch"

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
      <code className="rounded bg-fill-3 px-1.5 py-0.5 text-sm text-ink">
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
  const toggleSplit = useAppStore((s) => s.toggleSplit)
  const setTheme = useAppStore((s) => s.setTheme)
  const sidebarCollapsed = useAppStore((s) => s.sidebarCollapsed)
  const viewport = useAppStore((s) => s.viewport)
  const { newAgent } = useSessions()
  useGlobalSessionEvents()
  useViewportWidth(isBootstrapped)
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)
  const [searchModalOpen, setSearchModalOpen] = useState(false)
  const contentBlocked =
    route !== "chat" || (viewport !== "wide" && !sidebarCollapsed)

  useEffect(() => {
    if (!import.meta.env.DEV || isBrowserPreview()) return
    let cancelled = false
    let unlisten: (() => void) | undefined
    void listen<string>("qa-open-browser", (event) => {
      if (cancelled) return
      void (async () => {
        const store = useAppStore.getState()
        store.setRoute("chat")
        let sessionId = store.activeSessionId
        if (!sessionId) {
          try {
            const meta = await createSession(newAgentCreateInput())
            if (cancelled) return
            store.setActiveSessionId(meta.id, { panel: "closed" })
            sessionId = meta.id
          } catch {
            return
          }
        }
        const key = sessionScopeKey(sessionId)
        store.openToolBesideChat(sessionId, "browser")
        store.setBrowserOwnerSessionId(key)
        store.setBrowserSessionState(key, {
          started: true,
          loading: true,
          url: event.payload,
          loadError: null,
        })
        await new Promise((r) => window.setTimeout(r, 500))
        if (cancelled) return
        await browserOpen(event.payload)
      })()
    }).then((fn) => {
      if (cancelled) fn()
      else unlisten = fn
    })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    const onOpenInBrowser = (ev: Event) => {
      const url = (ev as CustomEvent<{ url?: string }>).detail?.url
      if (typeof url !== "string" || !url) return
      if (isBrowserPreview()) return
      void (async () => {
        const store = useAppStore.getState()
        const sessionId = store.activeSessionId
        if (!sessionId) return
        const key = sessionScopeKey(sessionId)
        const alreadyStarted = !!store.browserBySession[key]?.started
        store.openToolBesideChat(sessionId, "browser")
        store.setBrowserOwnerSessionId(key)
        store.setBrowserSessionState(key, {
          started: true,
          loading: true,
          url,
          loadError: null,
        })
        if (!alreadyStarted) {
          await new Promise((r) => window.setTimeout(r, 500))
        }
        try {
          await browserOpen(url)
        } catch {
        }
      })()
    }
    window.addEventListener("flex:open-in-browser", onOpenInBrowser)
    return () => window.removeEventListener("flex:open-in-browser", onOpenInBrowser)
  }, [])

  const handlersRef = useRef({
    onSend: () => {},
    onNewSession: () => {},
    onSearch: () => {},
    onFocusComposer: () => {},
    onCancel: (): boolean => false,
    onToggleSidebar: () => {},
    onToggleRightPanel: () => {},
    onToggleCommandPalette: () => {},
    onCloseActiveTab: () => {},
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
      toggleSplit()
    },
    onToggleCommandPalette: () => {
      setCommandPaletteOpen((v) => !v)
    },
    onCloseActiveTab: () => {
      const state = useAppStore.getState()
      if (state.route !== "chat") return
      const layout = state.contentLayout
      const pane = layout.panes[layout.focusedPane]
      if (!pane?.activeTabId) return
      state.closeTabInPane(layout.focusedPane, pane.activeTabId)
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
      const activeId = state.activeSessionId
      if (
        activeId &&
        ((state.pendingPermission &&
          state.pendingPermission.sessionId === activeId) ||
          (state.pendingQuestion &&
            state.pendingQuestion.sessionId === activeId))
      ) {
        return true
      }
      if (state.viewport !== "wide") {
        if (!state.sidebarCollapsed) {
          state.setSidebarCollapsed(true)
          return true
        }
      }
      if (!state.isStreaming || !state.activeSessionId) return false
      const sessionId = state.activeSessionId
      state.setIsStreaming(false)
      state.setSessionStreaming(sessionId, false)
      state.clearStreamingForSession(sessionId)
      state.requestSweep(sessionId)
      state.setSessionDraining(sessionId, true)
      void cancel(sessionId)
      return true
    },
  }

  useKeyboardShortcuts(handlersRef)

  useBootstrap(setRoute, setTheme)
  useUpdaterCheck(isBootstrapped && route !== "welcome")

  useEffect(() => {
    if (isBootstrapped) startDesktopIdlePrefetch()
  }, [isBootstrapped])

  const openCommandPalette = useCallback(() => setCommandPaletteOpen(true), [])
  const openSearch = useCallback(() => setSearchModalOpen(true), [])

  const titleBar = (
    <WindowTitleBar
      onOpenCommandPalette={openCommandPalette}
      onOpenSearch={openSearch}
    />
  )

  if (!isBootstrapped) {
    return (
      <div className="flex h-full flex-col">
        {titleBar}
        <div
          className="flex min-h-0 flex-1 cursor-default items-center justify-center gap-2 text-sm text-ink-muted"
          data-tauri-drag-region
          role="status"
          aria-live="polite"
        >
          <Spinner size="md" className="pointer-events-none" />
          <span className="pointer-events-none">Loading…</span>
        </div>
      </div>
    )
  }

  if (route === "welcome") {
    return (
      <div className="flex h-full flex-col">
        {titleBar}
        <div className="min-h-0 flex-1 overflow-auto">
          <WelcomePage />
        </div>
        <CommandPalette
          open={commandPaletteOpen}
          onClose={() => setCommandPaletteOpen(false)}
        />
        <ToastHost />
      </div>
    )
  }

  return (
    <div className="relative flex h-full min-h-0">
      <TitleBarChromeHost
        onOpenCommandPalette={openCommandPalette}
        onOpenSearch={openSearch}
      />
      <SessionSidebar onOpenSearch={() => setSearchModalOpen(true)} />
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
        <div
          className={cn(
            "flex h-full min-h-0 flex-1 flex-col",
            "transition-opacity duration-[var(--duration-normal)] ease-[var(--easing-default)] motion-reduce:transition-none",
            route !== "chat" && "pointer-events-none opacity-0",
          )}
          aria-hidden={contentBlocked}
          inert={contentBlocked}
        >
          <ContentWorkspace
            onOpenCommandPalette={openCommandPalette}
            onOpenSearch={openSearch}
          />
        </div>
        {route === "settings" ||
        route === "customize" ||
        (AUTOMATIONS_UI_ENABLED && route === "automations") ||
        route === "memory" ? (
          <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
            <Suspense
              fallback={
                <div
                  className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted"
                  role="status"
                  aria-live="polite"
                >
                  <Spinner size="md" />
                  Loading…
                </div>
              }
            >
              <SettingsPage embedded />
            </Suspense>
          </div>
        ) : null}
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

const useIdePlayground = (): boolean => {
  const [active, setActive] = useState(() =>
    typeof window !== "undefined" ? isIdePlaygroundHash() : false,
  )
  useEffect(() => {
    const sync = () => setActive(isIdePlaygroundHash())
    window.addEventListener("hashchange", sync)
    return () => window.removeEventListener("hashchange", sync)
  }, [])
  return active
}

const App = () => {
  const playground = useIdePlayground()
  if (playground) {
    return (
      <div className="h-full min-h-0 overflow-hidden">
        <IdePlayground />
      </div>
    )
  }
  if (isBrowserPreview()) return <NativeAppRequired />
  return (
    <QueryClientProvider client={queryClient}>
      <div className="app-shell agents-page flex h-full min-h-0 flex-col overflow-hidden">
        <AppRoutes />
      </div>
    </QueryClientProvider>
  )
}

export default App
