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
import { ContentWorkspace } from "./components/organisms/content/ContentWorkspace"
import { ToastHost } from "./components/molecules"
import { Spinner } from "./components/atoms"
import { WelcomePage } from "./pages/WelcomePage"
import { sessionScopeKey, useAppStore } from "./stores/appStore"
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
  const { newAgent } = useSessions()
  useGlobalSessionEvents()
  // Gated on isBootstrapped so the first classification runs after
  // restoreUiState applies the persisted sidebarCollapsed value — otherwise
  // an early narrow classification could force-collapse before the user's
  // saved preference is even loaded (BEHAVIOR SPEC #5).
  useViewportWidth(isBootstrapped)
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false)
  const [searchModalOpen, setSearchModalOpen] = useState(false)

  // Dev QA: Rust emits `qa-open-browser` when FLEX_BROWSER_QA=1 so we can
  // validate native webview bounds without flaky UI automation.
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
        // Let ContentPane + BrowserTab commit refs before open/bounds.
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

  // A web link clicked anywhere in chat markdown (see MarkdownBody's `a`)
  // opens in the embedded Browser panel — never the app's own webview (which
  // would replace the whole UI with the page). Opens/focuses the Browser tab
  // for the active session and navigates it.
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
        // First open: let ContentPane + BrowserTab mount/commit refs before the
        // native webview is created and positioned (mirrors qa-open-browser).
        if (!alreadyStarted) {
          await new Promise((r) => window.setTimeout(r, 500))
        }
        // `browserOpen` is idempotent — navigates the existing webview or
        // creates it (see browser.rs::browser_open).
        try {
          await browserOpen(url)
        } catch {
          /* surfaced as a toast via the browser session hook */
        }
      })()
    }
    window.addEventListener("flex:open-in-browser", onOpenInBrowser)
    return () => window.removeEventListener("flex:open-in-browser", onOpenInBrowser)
  }, [])

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
      toggleSplit()
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
      // HITL overlays own Esc while visible — never cancel the in-flight turn
      // underneath an Allow Bash / AskUserQuestion prompt (that looked like
      // "generation closed all modals").
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
      // Narrow/tight: an open sidebar overlay covers the chat area — Esc
      // closes it before falling through to turn-cancel. Split is wide-only.
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
      <div className="flex h-full flex-col bg-bg">
        {titleBar}
        <div
          className="flex min-h-0 flex-1 items-center justify-center gap-2 text-sm text-ink-muted"
          role="status"
          aria-live="polite"
        >
          <Spinner size="md" />
          Loading…
        </div>
      </div>
    )
  }

  if (route === "welcome") {
    return (
      <div className="flex h-full flex-col bg-bg">
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

  // Persistent sidebar + keep Chat mounted so timeline/subscriptions survive
  // settings round-trips (reference glass: content swap, not full remount).
  return (
    <div className="flex h-full flex-col bg-bg">
      {titleBar}
      <div className="relative flex min-h-0 flex-1">
        {/* Root is `relative` so SessionSidebar's mobile overlay (absolute,
         * anchored to this container's left edge — see SessionSidebar.tsx's
         * `narrow` handling) spans the full app width, not just the chat area. */}
        <SessionSidebar onOpenSearch={() => setSearchModalOpen(true)} />
        {/* Content workspace fills remaining width (tabs + optional split). */}
        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
          <div
            className={cn(
              "flex h-full min-h-0 flex-1 flex-col",
              "transition-opacity duration-[var(--duration-normal)] ease-[var(--easing-default)]",
              route !== "chat" && "pointer-events-none opacity-0",
            )}
            aria-hidden={route !== "chat"}
          >
            <ContentWorkspace />
          </div>
          {route === "settings" ||
          route === "customize" ||
          (AUTOMATIONS_UI_ENABLED && route === "automations") ||
          route === "memory" ? (
            <div className="absolute inset-0 flex min-h-0 flex-1 flex-col animate-pane-fade">
              <Suspense
                fallback={
                  <div className="flex h-full items-center justify-center gap-2 text-sm text-ink-muted">
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
