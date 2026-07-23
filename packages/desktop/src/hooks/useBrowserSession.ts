import { useCallback, useEffect, useRef } from "react"
import {
  browserBack,
  browserClearData,
  browserClose,
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
import { computeBrowserWebviewBounds } from "../lib/browserWebviewBounds"
import {
  BROWSER_BODY_OBSERVER,
  BROWSER_ROOT_OBSERVER,
  browserBoundsWatchdogMs,
} from "../lib/browserBoundsPolicy"

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

export const useBrowserSession = (active: boolean, sessionId: string | null) => {
  const sessionKey = sessionScopeKey(sessionId)

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
  const rightPanelDragging = useAppStore((s) => s.rightPanelDragging)

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

  const hostRef = useRef<HTMLDivElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const toolbarRef = useRef<HTMLDivElement>(null)
  const loadingTimeoutRef = useRef<number | null>(null)

  const clearLoadingSoon = useCallback(() => {
    if (loadingTimeoutRef.current !== null) {
      window.clearTimeout(loadingTimeoutRef.current)
    }
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
    if (!browserStarted || !isOwner) return
    setBrowserSessionState(sessionKey, { loading: true })
    clearLoadingSoon()
    void browserReload().catch((err) => {
      log.error("browser", "reload failed", { error: toInvokeError(err) })
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
    if (!browserUrl) return Promise.resolve()
    return navigator.clipboard
      .writeText(browserUrl)
      .then(() => pushToast("URL copied", "success"))
      .catch(() => pushToast("Couldn't copy URL", "error"))
  }, [browserUrl, pushToast])

  const handleClearHistory = useCallback(async () => {
    try {
      if (!isBrowserPreview() && isOwner && browserStarted) {
        await browserClose()
        setBrowserDesignMode(false)
      }
      resetBrowserSession(sessionKey)
      if (browserOwnerSessionId === sessionKey) {
        setBrowserOwnerSessionId(null)
      }
      pushToast("Browsing history cleared", "success")
    } catch (err) {
      const message = toInvokeError(err)
      log.error("browser", "clear history failed", { error: message })
      pushToast(message, "error")
    }
  }, [
    browserOwnerSessionId,
    browserStarted,
    isOwner,
    pushToast,
    resetBrowserSession,
    sessionKey,
    setBrowserDesignMode,
    setBrowserOwnerSessionId,
  ])

  const handleClearData = useCallback(async () => {
    if (isBrowserPreview()) {
      pushToast(NATIVE_APP_REQUIRED, "error")
      return
    }
    if (!browserStarted || !isOwner) {
      pushToast("Open a page before clearing browsing data", "error")
      return
    }
    try {
      await browserClearData()
      setBrowserSessionState(sessionKey, { loading: true })
      clearLoadingSoon()
      await browserHardReload()
      pushToast("Browsing data cleared", "success")
    } catch (err) {
      const message = toInvokeError(err)
      log.error("browser", "clear data failed", { error: message })
      pushToast(message, "error")
    }
  }, [
    browserStarted,
    clearLoadingSoon,
    isOwner,
    pushToast,
    sessionKey,
    setBrowserSessionState,
  ])

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
    if (!browserStarted || !isOwner) {
      pushToast("Open a page before opening DevTools", "error")
      return
    }
    void browserOpenDevtools().catch((err) => {
      pushToast(toInvokeError(err), "error")
    })
  }, [browserStarted, isOwner, pushToast])

  const handleBack = useCallback(() => {
    if (!browserStarted || !isOwner) return
    void browserBack().catch((err) => {
      log.error("browser", "back failed", { error: toInvokeError(err) })
      pushToast(toInvokeError(err), "error")
    })
  }, [browserStarted, isOwner, pushToast])

  const handleForward = useCallback(() => {
    if (!browserStarted || !isOwner) return
    void browserForward().catch((err) => {
      log.error("browser", "forward failed", { error: toInvokeError(err) })
      pushToast(toInvokeError(err), "error")
    })
  }, [browserStarted, isOwner, pushToast])

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
      // Quiet select: chip + composer focus are the affordance; a toast on
      // every inspect click is noise (Design Mode is intentionally silent).
      window.dispatchEvent(new CustomEvent("flex:focus-composer"))
    },
    [addAttachment, clearAttachments],
  )

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      unlisten = await listenBrowserState((e) => {
        const ownerKey = useAppStore.getState().browserOwnerSessionId
        if (!ownerKey) return
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
      void browserSetVisible(false)
      return
    }

    let cancelled = false
    let rafId: number | null = null
    let lastSent: { x: number; y: number; w: number; h: number } | null = null
    const sashEls = document.querySelectorAll<HTMLElement>(
      '[data-slot="resizable-handle"], [aria-label="Resize sessions sidebar"]',
    )
    const shouldHideWebview = (slotRect: DOMRectReadOnly) =>
      isNativeWebviewSuppressed(slotRect) ||
      useAppStore.getState().rightPanelDragging
    const measure = (force: boolean) => {
      if (cancelled) return
      const rect = slot.getBoundingClientRect()
      if (shouldHideWebview(rect)) {
        lastSent = null
        void browserSetVisible(false)
        return
      }
      const sashes = Array.from(sashEls)
        .map((el) => el.getBoundingClientRect())
        .filter((r) => r.width > 0 && r.height > 0)
        .map((r) => ({
          x: r.left,
          y: r.top,
          width: r.width,
          height: r.height,
        }))
      const presetWidth =
        VIEWPORT_PRESETS.find((p) => p.id === viewportPreset)?.width ?? null
      const next = computeBrowserWebviewBounds({
        slot: {
          x: rect.left,
          y: rect.top,
          width: rect.width,
          height: rect.height,
        },
        presetWidth,
        windowWidth: window.innerWidth,
        windowHeight: window.innerHeight,
        sashes,
      })
      if (!next) {
        lastSent = null
        void browserSetVisible(false)
        return
      }
      const { x, y, width, height } = next
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
        if (shouldHideWebview(slot.getBoundingClientRect())) {
          void browserSetVisible(false)
          return
        }
        void browserSetVisible(true)
      })
    }
    const schedule = () => {
      if (rafId !== null) return
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
    for (const el of sashEls) resizeObserver.observe(el)
    const onScroll = () => schedule()
    window.addEventListener("scroll", onScroll, true)
    // Live window resize throttles rAF inside WKWebView — hide immediately so
    // the OS edge grip / sash stay hittable, then reshow on the next measure.
    const onWindowResize = () => {
      lastSent = null
      void browserSetVisible(false)
      schedule()
    }
    window.addEventListener("resize", onWindowResize)
    schedule()
    // Streaming and idle share a 2s cadence — the previous 500ms idle
    // watchdog was overkill once dialogs no longer fan out via subtree.
    const watchdog = window.setInterval(
      () => measure(false),
      browserBoundsWatchdogMs(false),
    )
    // Cheap overlay detection: body direct children only (dialogs mount as
    // body-level portals), plus aria-modal / suppress flags on <html>.
    const overlayObserver = new MutationObserver(() => schedule())
    overlayObserver.observe(document.body, BROWSER_BODY_OBSERVER)
    overlayObserver.observe(document.documentElement, BROWSER_ROOT_OBSERVER)

    return () => {
      cancelled = true
      resizeObserver.disconnect()
      overlayObserver.disconnect()
      window.removeEventListener("scroll", onScroll, true)
      window.removeEventListener("resize", onWindowResize)
      window.clearInterval(watchdog)
      if (rafId !== null) cancelAnimationFrame(rafId)
      void browserSetVisible(false)
    }
  }, [
    active,
    isOwner,
    browserStarted,
    loadError,
    viewportPreset,
    rightPanelDragging,
  ])

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
    browserBack: handleBack,
    browserForward: handleForward,
  }
}
