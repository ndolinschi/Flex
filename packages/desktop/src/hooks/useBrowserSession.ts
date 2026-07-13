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
import { isNativeWebviewSuppressed } from "../lib/nativeWebviewGate"
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
 * Bounds map 1:1 to the empty `data-browser-webview-slot` (`contentRef`).
 * PRESERVES: the 500ms drift-watchdog + resize/scale reapply + reveal/hide
 * gating (see Effect 2 below) and all navigation behavior. */
export const useBrowserSession = (active: boolean) => {
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const sessionKey = sessionScopeKey(activeSessionId)

  const browserUrl = useAppStore(
    (s) => s.browserBySession[sessionKey]?.url ?? "",
  )
  const browserLoading = useAppStore(
    (s) => s.browserBySession[sessionKey]?.loading ?? false,
  )
  const browserStarted = useAppStore(
    (s) => s.browserBySession[sessionKey]?.started ?? false,
  )
  const viewportPreset = useAppStore(
    (s) => s.browserBySession[sessionKey]?.viewportPreset ?? "fill",
  )
  const loadError = useAppStore(
    (s) => s.browserBySession[sessionKey]?.loadError ?? null,
  )

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

  /** Browser chrome column (toolbar + body). Observed for resize only. */
  const hostRef = useRef<HTMLDivElement>(null)
  /** Empty `data-browser-webview-slot` — sole bounds source for the native
   * child webview (`getBoundingClientRect` → `browser_set_bounds`). */
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
    void browserOpenDevtools().catch((err) => {
      pushToast(toInvokeError(err), "error")
    })
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
      pushToast(
        additive
          ? `Added ${name} — describe the change`
          : `Selected ${name} — describe the change`,
        "success",
      )
    },
    [addAttachment, clearAttachments, pushToast],
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
  // Bounds are 1:1 with the empty `data-browser-webview-slot` rect. Spacing
  // (e.g. top gap) is CSS on that slot; Rust still stretches height to the
  // window bottom from the measured y so a short flex box can't leave a gap.
  //
  // Visibility is NOT gated on `browserLoading`. Hide only when the tab is
  // inactive, this session doesn't own the webview, the browser hasn't
  // started, a confirmed loadError, or a blocking HTML overlay is open
  // (native child webviews always paint above React).
  useEffect(() => {
    if (isBrowserPreview()) return
    const shouldShow = active && isOwner && browserStarted && !loadError
    if (!shouldShow) {
      void browserSetVisible(false)
      return
    }
    const slot = contentRef.current
    const host = hostRef.current
    if (!slot || !host) {
      // Refs not committed yet — never leave a stale visible webview up.
      void browserSetVisible(false)
      return
    }

    let cancelled = false
    let rafId: number | null = null
    // Last bounds sent to Rust — the watchdog only re-sends on real drift so
    // the 500ms interval never spams IPC when layout is stable.
    let lastSent: { x: number; y: number; w: number; h: number } | null = null
    const measure = (force: boolean) => {
      if (cancelled) return
      // Native child webviews sit above every HTML stacking context — hide
      // while modals/palettes claim the screen so they stay visible/clickable.
      if (isNativeWebviewSuppressed()) {
        lastSent = null
        void browserSetVisible(false)
        return
      }
      const rect = slot.getBoundingClientRect()
      if (rect.width < 2 || rect.height < 2) {
        lastSent = null
        void browserSetVisible(false)
        return
      }
      const widthSource = rect.width
      const presetWidth = VIEWPORT_PRESETS.find(
        (p) => p.id === viewportPreset,
      )?.width
      const width = presetWidth
        ? Math.min(presetWidth, widthSource)
        : widthSource
      const x = rect.left + (widthSource - width) / 2
      const y = rect.top
      const height = rect.height
      if (
        !force &&
        lastSent &&
        Math.abs(lastSent.x - x) < 0.5 &&
        Math.abs(lastSent.y - y) < 0.5 &&
        Math.abs(lastSent.w - width) < 0.5 &&
        Math.abs(lastSent.h - height) < 0.5
      ) {
        return
      }
      lastSent = { x, y, w: width, h: height }
      void browserSetBounds(x, y, width, height).then(() => {
        if (cancelled) return
        if (isNativeWebviewSuppressed()) {
          void browserSetVisible(false)
          return
        }
        void browserSetVisible(true)
      })
    }
    const schedule = () => {
      if (rafId !== null) return
      // Double-rAF so flex chrome has a committed layout before we read
      // the slot — a single rAF can still see pre-toolbar geometry.
      rafId = requestAnimationFrame(() => {
        rafId = requestAnimationFrame(() => {
          rafId = null
          measure(true)
        })
      })
    }

    const resizeObserver = new ResizeObserver(schedule)
    resizeObserver.observe(host)
    resizeObserver.observe(slot)
    const toolbar = toolbarRef.current
    if (toolbar) resizeObserver.observe(toolbar)
    const panelEl = document.querySelector<HTMLElement>(
      '[aria-label="Details panel"]',
    )
    if (panelEl) resizeObserver.observe(panelEl)
    window.addEventListener("resize", schedule)
    schedule()
    // Drift watchdog: ResizeObserver misses position-only moves (sidebar
    // toggle, narrow-overlay transitions) and rAF can be throttled/suspended
    // by WKWebView (live window resize, occlusion), leaving the native child
    // webview over the toolbar. A plain interval keeps firing in those cases;
    // measure(false) only re-sends when the rect actually drifted.
    const watchdog = window.setInterval(() => measure(false), 500)
    // Overlay open/close doesn't resize the content host — observe DOM so
    // CommandPalette / ConfirmDialog / etc. immediately hide the webview.
    const overlayObserver = new MutationObserver(() => measure(true))
    overlayObserver.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["aria-modal", "data-suppress-native-webview"],
    })

    return () => {
      cancelled = true
      resizeObserver.disconnect()
      overlayObserver.disconnect()
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
    hostRef,
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
