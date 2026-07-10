import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react"
import { ArrowLeft, ArrowRight, Globe, Loader2, RotateCw, Star } from "lucide-react"
import { IconButton } from "../atoms"
import {
  browserBack,
  browserForward,
  browserNavigate,
  browserOpen,
  browserReload,
  browserSetBounds,
  browserSetVisible,
  listenBrowserState,
} from "../../lib/tauri"
import { isBrowserPreview } from "../../lib/browserMock"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

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

/* ── Browser tab ──────────────────────────────────────────────────────── */

/** Cursor-style Browser right-panel tab: toolbar + omnibar + content area.
 * Stays mounted when inactive (parent hides via display:none). */
export const BrowserTab = ({ active }: { active: boolean }) => {
  const browserUrl = useAppStore((s) => s.browserUrl)
  const browserLoading = useAppStore((s) => s.browserLoading)
  const browserStarted = useAppStore((s) => s.browserStarted)
  const setBrowserState = useAppStore((s) => s.setBrowserState)

  const containerRef = useRef<HTMLDivElement>(null)
  const iframeRef = useRef<HTMLIFrameElement>(null)
  const [editing, setEditing] = useState(false)

  const commitNavigate = useCallback(
    (raw: string) => {
      const trimmed = raw.trim()
      setEditing(false)
      if (!trimmed) return
      if (browserStarted) {
        void browserNavigate(trimmed)
      } else {
        void browserOpen(trimmed)
      }
    },
    [browserStarted],
  )

  const handleInputKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault()
      commitNavigate(e.currentTarget.value)
    } else if (e.key === "Escape") {
      e.preventDefault()
      setEditing(false)
    }
  }

  // Effect 1: browser-state subscription (mount once).
  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | null = null

    const boot = async () => {
      unlisten = await listenBrowserState((e) => {
        setBrowserState({
          browserUrl: e.url,
          browserTitle: e.title,
          browserLoading: e.loading,
          browserStarted: true,
        })
        if (iframeRef.current && iframeRef.current.src !== e.url) {
          iframeRef.current.src = e.url
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
  }, [setBrowserState])

  // Effect 2: bounds sync (native only).
  useEffect(() => {
    if (isBrowserPreview()) return
    const container = containerRef.current
    if (!container) return

    let rafId: number | null = null
    const measure = () => {
      rafId = null
      const rect = container.getBoundingClientRect()
      void browserSetBounds(rect.left, rect.top, rect.width, rect.height)
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
  }, [active])

  // Effect 3: visibility (native only).
  useEffect(() => {
    if (isBrowserPreview()) return
    void browserSetVisible(active && browserStarted)
    return () => {
      void browserSetVisible(false)
    }
  }, [active, browserStarted])

  const preview = isBrowserPreview()

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Toolbar */}
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-stroke-3 px-1.5">
        <div className="flex items-center gap-px">
          <IconButton
            label="Back"
            disabled={!browserStarted}
            onClick={() => void browserBack()}
            className="h-6 w-6"
          >
            <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
          </IconButton>
          <IconButton
            label="Forward"
            disabled={!browserStarted}
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
                disabled={!browserStarted}
                onClick={() => void browserReload()}
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

        <IconButton
          label="Bookmark This Page"
          disabled
          className="h-6 w-6"
        >
          <Star className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
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
        ) : preview ? (
          <iframe
            ref={iframeRef}
            title="Browser"
            sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
            onLoad={() => setBrowserState({ browserLoading: false })}
            className="h-full w-full border-0 bg-white"
          />
        ) : (
          <div className="h-full w-full bg-black/20" />
        )}
      </div>
    </div>
  )
}
