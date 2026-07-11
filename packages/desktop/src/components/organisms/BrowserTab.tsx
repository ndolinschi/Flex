import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react"
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  Bug,
  Camera,
  Copy,
  Eraser,
  Globe,
  History,
  Loader2,
  Maximize,
  MoreHorizontal,
  Monitor,
  RotateCw,
  Smartphone,
  Tablet,
} from "lucide-react"
import { Button, IconButton, Tooltip } from "../atoms"
import {
  browserBack,
  browserClearData,
  browserForward,
  browserHardReload,
  browserNavigate,
  browserOpen,
  browserOpenDevtools,
  browserReload,
  browserScreenshot,
  browserSetBounds,
  browserSetVisible,
  listenBrowserState,
  toInvokeError,
} from "../../lib/tauri"
import { isBrowserPreview } from "../../lib/browserMock"
import {
  useAppStore,
  sessionScopeKey,
  type BrowserViewportPreset,
} from "../../stores/appStore"
import { cn } from "../../lib/utils"

/* ── Viewport presets ─────────────────────────────────────────────────── */

const VIEWPORT_PRESETS: Array<{
  id: BrowserViewportPreset
  label: string
  icon: typeof Smartphone
  width: number | null
}> = [
  { id: "mobile", label: "Mobile (375px)", icon: Smartphone, width: 375 },
  { id: "tablet", label: "Tablet (768px)", icon: Tablet, width: 768 },
  { id: "desktop", label: "Desktop (1280px)", icon: Monitor, width: 1280 },
  { id: "fill", label: "Fill", icon: Maximize, width: null },
]

/* ── Formatted URL (omnibar display mode) ────────────────────────────── */

const FormattedUrl = ({ url }: { url: string }) => {
  try {
    const parsed = new URL(url)
    const path = `${parsed.pathname}${parsed.search}`
    return (
      <span className="truncate">
        <span className="text-ink-muted opacity-50">{parsed.protocol}//</span>
        <span className="text-ink opacity-85">{parsed.host}</span>
        {path && path !== "/" ? (
          <span className="text-ink-secondary">{path}</span>
        ) : null}
      </span>
    )
  } catch {
    return <span className="truncate text-ink-secondary">{url}</span>
  }
}

/* ── "…" overflow menu row styling — 12px rows, matches ContextMenu. ──── */

const menuItemClass = cn(
  "flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-sm",
  "text-ink-secondary transition-colors hover:bg-fill-3 hover:text-ink",
  "disabled:pointer-events-none disabled:opacity-40",
)

/* ── Browser tab ──────────────────────────────────────────────────────── */

/** Browser right-panel tab: toolbar + omnibar + content area.
 * Scoped to the active session. Only one native webview / iframe exists;
 * `browserOwnerSessionId` tracks which session's content it currently shows.
 * Navigating from a session takes ownership. A session that previously
 * navigated but lost ownership shows a "Page is open in another chat" state
 * with a button to reclaim the webview. Stays mounted when inactive (parent
 * hides via display:none). */
export const BrowserTab = ({ active }: { active: boolean }) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const sessionKey = sessionScopeKey(activeSessionId)

  const browserBySession = useAppStore((s) => s.browserBySession)
  const sessionState = browserBySession[sessionKey]
  const browserUrl = sessionState?.url ?? ""
  const browserLoading = sessionState?.loading ?? false
  const browserStarted = sessionState?.started ?? false
  const viewportPreset = sessionState?.viewportPreset ?? "fill"
  const loadError = sessionState?.loadError ?? null

  const browserOwnerSessionId = useAppStore((s) => s.browserOwnerSessionId)
  const isOwner = browserOwnerSessionId === sessionKey

  const setBrowserSessionState = useAppStore((s) => s.setBrowserSessionState)
  const setBrowserOwnerSessionId = useAppStore(
    (s) => s.setBrowserOwnerSessionId,
  )
  const resetBrowserSession = useAppStore((s) => s.resetBrowserSession)
  const addAttachment = useAppStore((s) => s.addAttachment)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const pushToast = useAppStore((s) => s.pushToast)

  const containerRef = useRef<HTMLDivElement>(null)
  const loadingTimeoutRef = useRef<number | null>(null)
  const menuRootRef = useRef<HTMLDivElement>(null)
  const [editing, setEditing] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [showErrorDetails, setShowErrorDetails] = useState(false)

  const clearLoadingSoon = useCallback(() => {
    if (loadingTimeoutRef.current !== null) {
      window.clearTimeout(loadingTimeoutRef.current)
    }
    // Safety: if Finished never arrives (SPA / 1×1 webview), clear spinner.
    loadingTimeoutRef.current = window.setTimeout(() => {
      loadingTimeoutRef.current = null
      setBrowserSessionState(sessionKey, { loading: false })
    }, 8_000)
  }, [sessionKey, setBrowserSessionState])

  const commitNavigate = useCallback(
    (raw: string) => {
      const trimmed = raw.trim()
      setEditing(false)
      if (!trimmed) return
      setBrowserSessionState(sessionKey, { loading: true, url: trimmed })
      setBrowserOwnerSessionId(sessionKey)
      clearLoadingSoon()
      // #region agent log
      fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "34bae6",
        },
        body: JSON.stringify({
          sessionId: "34bae6",
          runId: "pre-fix",
          hypothesisId: "H5",
          location: "BrowserTab.tsx:commitNavigate",
          message: "navigate requested",
          data: {
            url: trimmed,
            browserStarted,
            preview: isBrowserPreview(),
            active,
          },
          timestamp: Date.now(),
        }),
      }).catch(() => {})
      // #endregion
      if (browserStarted) {
        void browserNavigate(trimmed).catch((err) => {
          // #region agent log
          fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "X-Debug-Session-Id": "34bae6",
            },
            body: JSON.stringify({
              sessionId: "34bae6",
              runId: "pre-fix",
              hypothesisId: "H5",
              location: "BrowserTab.tsx:browserNavigate",
              message: "navigate failed",
              data: { error: String(err) },
              timestamp: Date.now(),
            }),
          }).catch(() => {})
          // #endregion
        })
      } else {
        void browserOpen(trimmed).catch((err) => {
          // #region agent log
          fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "X-Debug-Session-Id": "34bae6",
            },
            body: JSON.stringify({
              sessionId: "34bae6",
              runId: "pre-fix",
              hypothesisId: "H5",
              location: "BrowserTab.tsx:browserOpen",
              message: "open failed",
              data: { error: String(err) },
              timestamp: Date.now(),
            }),
          }).catch(() => {})
          // #endregion
        })
      }
    },
    [
      active,
      browserStarted,
      clearLoadingSoon,
      sessionKey,
      setBrowserOwnerSessionId,
      setBrowserSessionState,
    ],
  )

  const handleReclaim = useCallback(() => {
    setBrowserOwnerSessionId(sessionKey)
    setBrowserSessionState(sessionKey, { loading: true })
    clearLoadingSoon()
    if (browserUrl) {
      void browserNavigate(browserUrl).catch(() => {})
    }
  }, [
    browserUrl,
    clearLoadingSoon,
    sessionKey,
    setBrowserOwnerSessionId,
    setBrowserSessionState,
  ])

  const handleInputKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault()
      commitNavigate(e.currentTarget.value)
    } else if (e.key === "Escape") {
      e.preventDefault()
      setEditing(false)
    }
  }

  /* ── "…" overflow menu actions ──────────────────────────────────────── */

  const handleScreenshot = useCallback(async () => {
    setMenuOpen(false)
    if (isBrowserPreview()) {
      pushToast("Screenshot unavailable in preview", "success")
      return
    }
    try {
      const path = await browserScreenshot()
      const name = path.split(/[/\\]/).pop() ?? path
      addAttachment({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        path,
        kind: "image",
        name,
      })
      pushToast("Screenshot attached to composer", "success")
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    }
  }, [addAttachment, pushToast])

  const handleHardReload = useCallback(() => {
    setMenuOpen(false)
    if (!browserStarted || !isOwner) return
    setBrowserSessionState(sessionKey, { loading: true })
    clearLoadingSoon()
    void browserHardReload().catch((err) => {
      pushToast(toInvokeError(err), "error")
    })
  }, [
    browserStarted,
    clearLoadingSoon,
    isOwner,
    pushToast,
    sessionKey,
    setBrowserSessionState,
  ])

  const handleCopyUrl = useCallback(() => {
    setMenuOpen(false)
    if (!browserUrl) return
    void navigator.clipboard
      .writeText(browserUrl)
      .then(() => pushToast("URL copied", "success"))
      .catch(() => pushToast("Couldn't copy URL", "error"))
  }, [browserUrl, pushToast])

  const handleClearHistory = useCallback(() => {
    setMenuOpen(false)
    resetBrowserSession(sessionKey)
    if (browserOwnerSessionId === sessionKey) {
      setBrowserOwnerSessionId(null)
    }
    pushToast("Browsing history cleared", "success")
  }, [
    browserOwnerSessionId,
    pushToast,
    resetBrowserSession,
    sessionKey,
    setBrowserOwnerSessionId,
  ])

  const handleClearData = useCallback(async () => {
    setMenuOpen(false)
    if (isBrowserPreview()) {
      pushToast("Clear Browsing Data unavailable in preview", "success")
      return
    }
    try {
      await browserClearData()
      pushToast("Browsing data cleared", "success")
    } catch (err) {
      pushToast(toInvokeError(err), "error")
    }
  }, [pushToast])

  const handleAskAgent = useCallback(() => {
    if (!browserUrl) return
    const message = loadError
      ? `The embedded browser failed to load ${browserUrl}: ${loadError.message}. Diagnose and fix.`
      : `The embedded browser failed to load ${browserUrl}. Diagnose and fix.`
    setComposerDraft(message)
    window.dispatchEvent(new CustomEvent("flex:focus-composer"))
  }, [browserUrl, loadError, setComposerDraft])

  // Effect 1: browser-state subscription (mount once). Applies to whichever
  // session currently owns the webview, not necessarily the viewed session.
  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      unlisten = await listenBrowserState((e) => {
        const ownerKey = useAppStore.getState().browserOwnerSessionId
        if (!ownerKey) return
        useAppStore.getState().setBrowserSessionState(ownerKey, {
          url: e.url,
          title: e.title,
          loading: e.loading,
          started: true,
          loadError: e.error ?? null,
        })
        if (!e.loading && loadingTimeoutRef.current !== null) {
          window.clearTimeout(loadingTimeoutRef.current)
          loadingTimeoutRef.current = null
        }
        if (e.loading) {
          if (loadingTimeoutRef.current !== null) {
            window.clearTimeout(loadingTimeoutRef.current)
          }
          loadingTimeoutRef.current = window.setTimeout(() => {
            loadingTimeoutRef.current = null
            useAppStore.getState().setBrowserSessionState(ownerKey, {
              loading: false,
            })
          }, 8_000)
        }
      })
      if (cancelled) {
        unlisten()
        unlisten = null
      }
    }

    void boot()

    return () => {
      cancelled = true
      if (unlisten) unlisten()
      if (loadingTimeoutRef.current !== null) {
        window.clearTimeout(loadingTimeoutRef.current)
      }
    }
  }, [])

  // Overflow menu: close on outside click / Escape (mirrors SessionMenu).
  useEffect(() => {
    if (!menuOpen) return
    const handlePointer = (e: MouseEvent) => {
      if (!menuRootRef.current?.contains(e.target as Node)) setMenuOpen(false)
    }
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        setMenuOpen(false)
      }
    }
    document.addEventListener("mousedown", handlePointer)
    document.addEventListener("keydown", handleKey)
    return () => {
      document.removeEventListener("mousedown", handlePointer)
      document.removeEventListener("keydown", handleKey)
    }
  }, [menuOpen])

  // Collapse the expanded error details whenever a fresh error page renders.
  useEffect(() => {
    setShowErrorDetails(false)
  }, [loadError])

  // Effect 2: bounds sync (native only) — re-run when webview starts or tab activates.
  useEffect(() => {
    if (isBrowserPreview()) return
    if (!active || !isOwner || !browserStarted) return
    const container = containerRef.current
    if (!container) return

    let rafId: number | null = null
    const measure = () => {
      rafId = null
      const rect = container.getBoundingClientRect()
      // #region agent log
      fetch("http://127.0.0.1:7399/ingest/4642b0a4-a520-4891-a625-7f347f2070b9", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Debug-Session-Id": "34bae6",
        },
        body: JSON.stringify({
          sessionId: "34bae6",
          runId: "pre-fix",
          hypothesisId: "H6",
          location: "BrowserTab.tsx:bounds",
          message: "measuring container",
          data: {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
            skipped: rect.width < 2 || rect.height < 2,
          },
          timestamp: Date.now(),
        }),
      }).catch(() => {})
      // #endregion
      if (rect.width < 2 || rect.height < 2) return
      // Viewport preset: clamp the reported rect to the preset width (never
      // wider than the panel) and center it horizontally, letterboxing the
      // remainder — the panel background shows through on both sides. Pure
      // frontend constraint: the existing bounds-reporting call just gets a
      // narrower/centered rect, no Rust changes needed.
      const presetWidth = VIEWPORT_PRESETS.find(
        (p) => p.id === viewportPreset,
      )?.width
      const width = presetWidth ? Math.min(presetWidth, rect.width) : rect.width
      const x = rect.left + (rect.width - width) / 2
      void browserSetBounds(x, rect.top, width, rect.height)
    }
    const schedule = () => {
      if (rafId !== null) return
      rafId = requestAnimationFrame(measure)
    }

    const resizeObserver = new ResizeObserver(schedule)
    resizeObserver.observe(container)
    window.addEventListener("resize", schedule)
    schedule()

    return () => {
      resizeObserver.disconnect()
      window.removeEventListener("resize", schedule)
      if (rafId !== null) cancelAnimationFrame(rafId)
    }
  }, [active, isOwner, browserStarted, viewportPreset])

  // Effect 3: visibility (native only) — only the owning, active session shows
  // the webview, and only while there's no load-error page covering it (the
  // in-panel error state replaces the raw webview rather than layering over it).
  useEffect(() => {
    if (isBrowserPreview()) return
    void browserSetVisible(active && isOwner && browserStarted && !loadError)
    return () => {
      void browserSetVisible(false)
    }
  }, [active, isOwner, browserStarted, loadError])

  const preview = isBrowserPreview()
  const showLiveContent = browserStarted && isOwner
  const showElsewhere = browserStarted && !isOwner
  const presetWidth = VIEWPORT_PRESETS.find((p) => p.id === viewportPreset)?.width

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Toolbar */}
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-stroke-3 px-1.5">
        <div className="flex items-center gap-px">
          <IconButton
            label="Back"
            disabled={!showLiveContent}
            onClick={() => void browserBack()}
            className="h-6 w-6"
          >
            <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
          <IconButton
            label="Forward"
            disabled={!showLiveContent}
            onClick={() => void browserForward()}
            className="h-6 w-6"
          >
            <ArrowRight className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
          <div className="relative flex h-6 w-6 items-center justify-center">
            {browserLoading ? (
              <Loader2
                className="h-3.5 w-3.5 animate-spin text-ink-muted"
                aria-hidden
              />
            ) : (
              <IconButton
                label="Reload"
                disabled={!showLiveContent}
                onClick={() => {
                  setBrowserSessionState(sessionKey, { loading: true })
                  clearLoadingSoon()
                  void browserReload()
                }}
                className="h-6 w-6"
              >
                <RotateCw className="h-3.5 w-3.5" aria-hidden />
              </IconButton>
            )}
          </div>
        </div>

        {/* Omnibar */}
        <div className="relative min-w-0 flex-1">
          {editing ? (
            <input
              autoFocus
              defaultValue={browserUrl}
              onKeyDown={handleInputKeyDown}
              onBlur={() => setEditing(false)}
              className="w-full rounded-sm bg-fill-4 px-2 py-1 text-sm text-ink outline-none"
            />
          ) : (
            <button
              type="button"
              onClick={() => setEditing(true)}
              className={cn(
                "flex w-full items-center truncate rounded-sm px-2 py-1 text-left text-sm",
                "cursor-text transition-colors hover:bg-fill-4",
              )}
            >
              {browserStarted ? (
                <FormattedUrl url={browserUrl} />
              ) : (
                <span className="text-ink-muted">Search or enter a URL</span>
              )}
            </button>
          )}
        </div>

        {/* Screen-size presets */}
        <div className="flex items-center gap-px">
          {VIEWPORT_PRESETS.map(({ id, label, icon: Icon }) => (
            <Tooltip key={id} label={label}>
              <IconButton
                label={label}
                onClick={() =>
                  setBrowserSessionState(sessionKey, { viewportPreset: id })
                }
                className={cn(
                  "h-6 w-6",
                  viewportPreset === id && "bg-surface-muted text-ink",
                )}
              >
                <Icon className="h-3.5 w-3.5" aria-hidden />
              </IconButton>
            </Tooltip>
          ))}
        </div>

        <Tooltip label="Open DevTools">
          <IconButton
            label="Open DevTools"
            onClick={() => {
              if (preview) {
                pushToast("DevTools unavailable in preview", "success")
                return
              }
              void browserOpenDevtools()
            }}
            className="h-6 w-6"
          >
            <Bug className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
        </Tooltip>

        {/* "…" overflow menu */}
        <div ref={menuRootRef} className="relative">
          <IconButton
            label="More browser actions"
            onClick={() => setMenuOpen((v) => !v)}
            className={cn("h-6 w-6", menuOpen && "bg-fill-3 text-ink")}
          >
            <MoreHorizontal className="h-3.5 w-3.5" aria-hidden />
          </IconButton>

          {menuOpen ? (
            <div
              role="menu"
              aria-label="Browser actions"
              className={cn(
                "absolute right-0 top-full z-50 mt-1 w-56 overflow-hidden rounded-lg",
                "border border-stroke-2 bg-panel py-0.5 shadow-lg animate-tray-in",
              )}
            >
              <button
                type="button"
                role="menuitem"
                disabled={!browserStarted}
                className={menuItemClass}
                onClick={() => void handleScreenshot()}
              >
                <Camera className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Take Screenshot
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!showLiveContent}
                className={menuItemClass}
                onClick={handleHardReload}
              >
                <RotateCw className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Hard Reload
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!browserUrl}
                className={menuItemClass}
                onClick={handleCopyUrl}
              >
                <Copy className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Copy Current URL
              </button>
              <div className="mx-2 my-0.5 border-t border-stroke-3" />
              <button
                type="button"
                role="menuitem"
                disabled={!browserStarted}
                className={menuItemClass}
                onClick={handleClearHistory}
              >
                <History className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Clear Browsing History
              </button>
              <button
                type="button"
                role="menuitem"
                className={menuItemClass}
                onClick={() => void handleClearData()}
              >
                <Eraser className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Clear Browsing Data
              </button>
            </div>
          ) : null}
        </div>
      </div>

      {/* Content */}
      <div ref={containerRef} className="relative min-h-0 flex-1">
        {!browserStarted ? (
          <div className="flex h-full flex-col items-center justify-center gap-2">
            <Globe className="h-8 w-8 text-ink-faint opacity-60" aria-hidden />
            <p className="text-[14px] font-medium text-ink">Browser</p>
            <p className="max-w-[300px] text-center text-sm text-ink-muted">
              Enter a URL above, or instruct the Agent to navigate and use the
              browser
            </p>
          </div>
        ) : showElsewhere ? (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <p className="max-w-[280px] text-center text-sm text-ink-muted">
              Page is open in another chat
            </p>
            <Button variant="secondary" size="sm" onClick={handleReclaim}>
              Reload here
            </Button>
          </div>
        ) : showLiveContent && loadError ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-6">
            <AlertTriangle className="h-8 w-8 text-danger opacity-80" aria-hidden />
            <p className="text-[14px] font-medium text-ink">
              Can't connect to server
            </p>
            <p className="max-w-[320px] text-center text-sm text-ink-muted">
              {loadError.message}
            </p>
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" onClick={handleAskAgent}>
                Ask Agent
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setShowErrorDetails((v) => !v)}
              >
                {showErrorDetails ? "Hide Details" : "Show Details"}
              </Button>
            </div>
            {showErrorDetails ? (
              <pre className="max-w-[420px] overflow-x-auto rounded-md bg-fill-3 px-3 py-2 text-left text-xs text-ink-muted">
                {`GET ${browserUrl}\n${loadError.host} refused to connect\n${loadError.message}`}
              </pre>
            ) : null}
          </div>
        ) : preview ? (
          <div className="flex h-full w-full items-stretch justify-center bg-fill-3">
            <iframe
              title="Browser"
              src={browserUrl || undefined}
              sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
              onLoad={() => setBrowserSessionState(sessionKey, { loading: false })}
              className="h-full border-0 bg-white"
              style={{
                width: presetWidth ? `min(${presetWidth}px, 100%)` : "100%",
              }}
            />
          </div>
        ) : (
          <div className="h-full w-full bg-black/20" />
        )}
      </div>
    </div>
  )
}
