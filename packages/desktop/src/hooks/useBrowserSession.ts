import { useCallback, useEffect, useRef } from "react"
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
  browserSetDesignMode,
  browserSetVisible,
  listenBrowserDesign,
  listenBrowserState,
  toInvokeError,
} from "../lib/tauri"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../lib/browserPreview"
import {
  useAppStore,
  sessionScopeKey,
  type BrowserViewportPreset,
} from "../stores/appStore"
import { log } from "../lib/debug/log"
import type { BrowserDomElement } from "../lib/browserDesign"

/* ── Viewport presets ─────────────────────────────────────────────────── */

export const VIEWPORT_PRESETS: Array<{
  id: BrowserViewportPreset
  label: string
  width: number | null
}> = [
  { id: "mobile", label: "Mobile (375px)", width: 375 },
  { id: "tablet", label: "Tablet (768px)", width: 768 },
  { id: "desktop", label: "Desktop (1280px)", width: 1280 },
  { id: "fill", label: "Fill", width: null },
]

/** Browser session/webview-ownership logic for the Browser right-panel tab.
 * Extracted from `BrowserTab.tsx` — owns the child-webview lifecycle, bounds
 * watchdog, navigation state, session ownership, and toast side effects.
 * `BrowserTab.tsx` remains the chrome view and consumes this hook.
 *
 * PRESERVES exactly: the 500ms drift-watchdog + resize/scale reapply +
 * reveal/hide gating (see Effect 2 below) and all navigation behavior. */
export const useBrowserSession = (active: boolean) => {
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
  const browserDesignMode = useAppStore((s) => s.browserDesignMode)

  const setBrowserSessionState = useAppStore((s) => s.setBrowserSessionState)
  const setBrowserOwnerSessionId = useAppStore(
    (s) => s.setBrowserOwnerSessionId,
  )
  const setBrowserDesignMode = useAppStore((s) => s.setBrowserDesignMode)
  const resetBrowserSession = useAppStore((s) => s.resetBrowserSession)
  const addAttachment = useAppStore((s) => s.addAttachment)
  const clearAttachments = useAppStore((s) => s.clearAttachments)
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const pushToast = useAppStore((s) => s.pushToast)

  /** Content area only — never the toolbar. Native webview bounds are taken
   * from this rect so the OS child layer cannot cover React chrome. */
  const contentRef = useRef<HTMLDivElement>(null)
  const toolbarRef = useRef<HTMLDivElement>(null)
  const loadingTimeoutRef = useRef<number | null>(null)

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
      if (!trimmed) return
      setBrowserSessionState(sessionKey, {
        loading: true,
        url: trimmed,
        loadError: null,
      })
      setBrowserOwnerSessionId(sessionKey)
      clearLoadingSoon()
      if (browserStarted) {
        void browserNavigate(trimmed).catch((err) => {
          log.error("browser", "navigate failed", {
            url: trimmed,
            error: toInvokeError(err),
          })
        })
      } else {
        void browserOpen(trimmed).catch((err) => {
          log.error("browser", "open failed", {
            url: trimmed,
            error: toInvokeError(err),
          })
        })
      }
    },
    [
      browserStarted,
      clearLoadingSoon,
      sessionKey,
      setBrowserOwnerSessionId,
      setBrowserSessionState,
    ],
  )

  const handleReclaim = useCallback(() => {
    setBrowserOwnerSessionId(sessionKey)
    setBrowserSessionState(sessionKey, { loading: true, loadError: null })
    clearLoadingSoon()
    if (browserUrl) {
      void browserNavigate(browserUrl).catch((err) => {
        log.error("browser", "navigate failed", {
          url: browserUrl,
          error: toInvokeError(err),
        })
      })
    }
  }, [
    browserUrl,
    clearLoadingSoon,
    sessionKey,
    setBrowserOwnerSessionId,
    setBrowserSessionState,
  ])

  /* ── "…" overflow menu actions ──────────────────────────────────────── */

  const handleScreenshot = useCallback(async () => {
    if (isBrowserPreview()) {
      pushToast(NATIVE_APP_REQUIRED, "error")
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
      const message = toInvokeError(err)
      log.error("browser", "screenshot failed", { error: message })
      pushToast(message, "error")
    }
  }, [addAttachment, pushToast])

  const handleHardReload = useCallback(() => {
    if (!browserStarted || !isOwner) return
    setBrowserSessionState(sessionKey, { loading: true })
    clearLoadingSoon()
    void browserHardReload().catch((err) => {
      log.error("browser", "hard reload failed", { error: toInvokeError(err) })
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

  const handleReload = useCallback(() => {
    setBrowserSessionState(sessionKey, { loading: true })
    clearLoadingSoon()
    void browserReload()
  }, [clearLoadingSoon, sessionKey, setBrowserSessionState])

  const handleCopyUrl = useCallback(() => {
    if (!browserUrl) return Promise.resolve()
    return navigator.clipboard
      .writeText(browserUrl)
      .then(() => pushToast("URL copied", "success"))
      .catch(() => pushToast("Couldn't copy URL", "error"))
  }, [browserUrl, pushToast])

  const handleClearHistory = useCallback(() => {
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
    if (isBrowserPreview()) {
      pushToast(NATIVE_APP_REQUIRED, "error")
      return
    }
    try {
      await browserClearData()
      pushToast("Browsing data cleared", "success")
    } catch (err) {
      const message = toInvokeError(err)
      log.error("browser", "clear data failed", { error: message })
      pushToast(message, "error")
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

  const setViewportPreset = useCallback(
    (id: BrowserViewportPreset) => {
      setBrowserSessionState(sessionKey, { viewportPreset: id })
    },
    [sessionKey, setBrowserSessionState],
  )

  const handleOpenDevtools = useCallback(() => {
    if (isBrowserPreview()) {
      pushToast(NATIVE_APP_REQUIRED, "error")
      return
    }
    void browserOpenDevtools()
  }, [pushToast])

  const toggleDesignMode = useCallback(async () => {
    if (!browserStarted || !isOwner) {
      pushToast("Open a page before using Design Mode", "error")
      return
    }
    if (loadError) {
      pushToast("Design Mode needs a loaded page", "error")
      return
    }
    const next = !useAppStore.getState().browserDesignMode
    try {
      await browserSetDesignMode(next)
      setBrowserDesignMode(next)
      if (next) {
        window.dispatchEvent(new CustomEvent("flex:focus-composer"))
      }
    } catch (err) {
      const message = toInvokeError(err)
      log.error("browser", "design mode toggle failed", { error: message })
      pushToast(message, "error")
    }
  }, [
    browserStarted,
    isOwner,
    loadError,
    pushToast,
    setBrowserDesignMode,
  ])

  const addDomChip = useCallback(
    (name: string, element: BrowserDomElement, additive: boolean) => {
      if (!additive) {
        // Replace prior DOM chips; keep image/file attachments.
        const keep = useAppStore
          .getState()
          .attachments.filter((a) => a.kind !== "dom")
        clearAttachments()
        for (const att of keep) addAttachment(att)
      }
      addAttachment({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        kind: "dom",
        name,
        payload: element,
      })
      window.dispatchEvent(new CustomEvent("flex:focus-composer"))
    },
    [addAttachment, clearAttachments],
  )

  // Effect 1: browser-state subscription (mount once). Applies to whichever
  // session currently owns the webview, not necessarily the viewed session.
  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      unlisten = await listenBrowserState((e) => {
        const ownerKey = useAppStore.getState().browserOwnerSessionId
        if (!ownerKey) return
        // Clear loadError on loading pulses; set it when native emits error.
        // Title-only pulses may omit `error` — don't clobber an existing one
        // unless loading started or Finished reported success/failure.
        const patch: {
          url: string
          title: string | null
          loading: boolean
          started: true
          loadError?: { host: string; message: string } | null
        } = {
          url: e.url,
          title: e.title,
          loading: e.loading,
          started: true,
        }
        if (e.loading) {
          patch.loadError = null
        } else if (e.error) {
          patch.loadError = e.error
        } else if (e.title == null) {
          // Page-load Finished emits title: null — clear prior error on success.
          patch.loadError = null
        }
        useAppStore.getState().setBrowserSessionState(ownerKey, patch)
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

  // Design Mode events → composer DOM chips; Escape-exit syncs the toolbar flag.
  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null
    const boot = async () => {
      unlisten = await listenBrowserDesign((e) => {
        if (e.type === "exit") {
          useAppStore.getState().setBrowserDesignMode(false)
          return
        }
        if (e.type === "select") {
          addDomChip(e.name, e.element, e.additive)
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
    }
  }, [addDomChip])

  // ⌘⇧D / Ctrl⇧D toggles Design Mode when the Browser tab is active.
  useEffect(() => {
    if (!active) return
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || !e.shiftKey) return
      if (e.key !== "d" && e.key !== "D") return
      e.preventDefault()
      void toggleDesignMode()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [active, toggleDesignMode])

  // Effect 2: bounds sync (native only) — re-run when webview starts or tab activates.
  // Owns reveal too: the webview must never be shown before its first real
  // (non-degenerate) rect has been applied, otherwise it paints at whatever
  // stale position it last had (e.g. (0,0) from creation), which sits on top
  // of the toolbar.
  //
  // Visibility is NOT gated on `browserLoading`. Hiding for the entire load
  // left a permanent black void when Finished/probe races never cleared
  // loading, while the page had already painted under the OS webview.
  // Hide only when the tab is inactive, this session doesn't own the
  // webview, the browser hasn't started, or a confirmed loadError (so the
  // React error page isn't covered by Chromium's sheet).
  useEffect(() => {
    if (isBrowserPreview()) return
    const shouldShow = active && isOwner && browserStarted && !loadError
    if (!shouldShow) {
      void browserSetVisible(false)
      return
    }
    const content = contentRef.current
    const toolbar = toolbarRef.current
    if (!content || !toolbar) return

    let cancelled = false
    let rafId: number | null = null
    // Last bounds sent to Rust — the watchdog only re-sends on real drift so
    // the 500ms interval never spams IPC when layout is stable.
    let lastSent: { x: number; y: number; w: number; h: number } | null = null
    const measure = (force: boolean) => {
      if (cancelled) return
      const contentRect = content.getBoundingClientRect()
      const toolbarRect = toolbar.getBoundingClientRect()
      // Authoritative top = toolbar bottom (border box includes padding).
      // Never trust contentRect.top alone — after display:none → flex, the
      // content area can briefly report the panel-tabs bottom (y≈69) and the
      // native layer paints over the h-9 chrome row.
      const top = Math.max(contentRect.top, toolbarRect.bottom)
      const height = contentRect.bottom - top
      if (contentRect.width < 2 || height < 2 || toolbarRect.height < 1) {
        lastSent = null
        void browserSetVisible(false)
        return
      }
      // Viewport preset: clamp width (never wider than the content area) and
      // center horizontally; panel bg letterboxes the sides.
      const presetWidth = VIEWPORT_PRESETS.find(
        (p) => p.id === viewportPreset,
      )?.width
      const width = presetWidth
        ? Math.min(presetWidth, contentRect.width)
        : contentRect.width
      const x = contentRect.left + (contentRect.width - width) / 2
      if (
        !force &&
        lastSent &&
        Math.abs(lastSent.x - x) < 0.5 &&
        Math.abs(lastSent.y - top) < 0.5 &&
        Math.abs(lastSent.w - width) < 0.5 &&
        Math.abs(lastSent.h - height) < 0.5
      ) {
        return
      }
      lastSent = { x, y: top, w: width, h: height }
      void browserSetBounds(x, top, width, height).then(() => {
        if (cancelled) return
        void browserSetVisible(true)
      })
    }
    const schedule = () => {
      if (rafId !== null) return
      // Double-rAF so flex chrome has a committed layout before we read
      // tops — a single rAF can still see pre-toolbar geometry.
      rafId = requestAnimationFrame(() => {
        rafId = requestAnimationFrame(() => {
          rafId = null
          measure(true)
        })
      })
    }

    const resizeObserver = new ResizeObserver(schedule)
    resizeObserver.observe(content)
    resizeObserver.observe(toolbar)
    window.addEventListener("resize", schedule)
    schedule()
    // Drift watchdog: ResizeObserver misses position-only moves (sidebar
    // toggle, narrow-overlay transitions) and rAF can be throttled/suspended
    // by WKWebView (live window resize, occlusion), leaving the native child
    // webview over the toolbar. A plain interval keeps firing in those cases;
    // measure(false) only re-sends when the rect actually drifted.
    const watchdog = window.setInterval(() => measure(false), 500)

    return () => {
      cancelled = true
      resizeObserver.disconnect()
      window.removeEventListener("resize", schedule)
      window.clearInterval(watchdog)
      if (rafId !== null) cancelAnimationFrame(rafId)
      void browserSetVisible(false)
    }
  }, [active, isOwner, browserStarted, loadError, viewportPreset])

  const preview = isBrowserPreview()
  const showLiveContent = browserStarted && isOwner
  const showElsewhere = browserStarted && !isOwner
  const presetWidth = VIEWPORT_PRESETS.find((p) => p.id === viewportPreset)?.width

  return {
    sessionKey,
    contentRef,
    toolbarRef,
    browserUrl,
    browserLoading,
    browserStarted,
    viewportPreset,
    setViewportPreset,
    loadError,
    isOwner,
    preview,
    showLiveContent,
    showElsewhere,
    presetWidth,
    commitNavigate,
    handleReclaim,
    handleScreenshot,
    handleHardReload,
    handleReload,
    handleCopyUrl,
    handleClearHistory,
    handleClearData,
    handleAskAgent,
    handleOpenDevtools,
    browserDesignMode,
    toggleDesignMode,
    setBrowserSessionState,
    browserBack: () => void browserBack(),
    browserForward: () => void browserForward(),
  }
}
