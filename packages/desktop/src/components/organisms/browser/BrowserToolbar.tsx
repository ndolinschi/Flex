import {
  useEffect,
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
} from "react"
import {
  ArrowLeft,
  ArrowRight,
  Bug,
  Loader2,
  Maximize,
  MoreHorizontal,
  Monitor,
  RotateCw,
  Smartphone,
  Tablet,
} from "lucide-react"
import { IconButton, Tooltip } from "../../atoms"
import { VIEWPORT_PRESETS as VIEWPORT_PRESETS_BASE } from "../../../hooks/useBrowserSession"
import type { BrowserViewportPreset } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import { BrowserOverflowMenu } from "./BrowserOverflowMenu"

const PRESET_ICONS: Record<BrowserViewportPreset, typeof Smartphone> = {
  mobile: Smartphone,
  tablet: Tablet,
  desktop: Monitor,
  fill: Maximize,
}

const VIEWPORT_PRESETS: Array<{
  id: BrowserViewportPreset
  label: string
  icon: typeof Smartphone
  width: number | null
}> = VIEWPORT_PRESETS_BASE.map((p) => ({ ...p, icon: PRESET_ICONS[p.id] }))

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

export type BrowserToolbarProps = {
  toolbarRef: RefObject<HTMLDivElement | null>
  browserUrl: string
  browserLoading: boolean
  browserStarted: boolean
  showLiveContent: boolean
  viewportPreset: BrowserViewportPreset
  setViewportPreset: (id: BrowserViewportPreset) => void
  editing: boolean
  setEditing: (v: boolean) => void
  menuOpen: boolean
  setMenuOpen: (v: boolean | ((prev: boolean) => boolean)) => void
  commitNavigate: (url: string) => void
  browserBack: () => void
  browserForward: () => void
  handleReload: () => void
  handleOpenDevtools: () => void
  handleScreenshot: () => void | Promise<void>
  handleHardReload: () => void
  handleCopyUrl: () => void | Promise<void>
  handleClearHistory: () => void
  handleClearData: () => void | Promise<void>
}

/** Browser chrome: nav buttons, omnibar, viewport presets, overflow menu. */
export const BrowserToolbar = ({
  toolbarRef,
  browserUrl,
  browserLoading,
  browserStarted,
  showLiveContent,
  viewportPreset,
  setViewportPreset,
  editing,
  setEditing,
  menuOpen,
  setMenuOpen,
  commitNavigate,
  browserBack,
  browserForward,
  handleReload,
  handleOpenDevtools,
  handleScreenshot,
  handleHardReload,
  handleCopyUrl,
  handleClearHistory,
  handleClearData,
}: BrowserToolbarProps) => {
  const menuRootRef = useRef<HTMLDivElement>(null)

  const handleInputKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault()
      setEditing(false)
      commitNavigate(e.currentTarget.value)
    } else if (e.key === "Escape") {
      e.preventDefault()
      setEditing(false)
    }
  }

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
  }, [menuOpen, setMenuOpen])

  return (
    <div
      ref={toolbarRef}
      className="relative z-20 flex h-9 min-h-9 shrink-0 items-center gap-1 overflow-hidden border-b border-stroke-3 bg-bg px-1.5"
    >
      <div className="flex items-center gap-px">
        <IconButton
          label="Back"
          disabled={!showLiveContent}
          onClick={browserBack}
          className="h-6 w-6"
        >
          <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
        <IconButton
          label="Forward"
          disabled={!showLiveContent}
          onClick={browserForward}
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
              onClick={handleReload}
              className="h-6 w-6"
            >
              <RotateCw className="h-3.5 w-3.5" aria-hidden />
            </IconButton>
          )}
        </div>
      </div>

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

      <div className="flex items-center gap-px">
        {VIEWPORT_PRESETS.map(({ id, label, icon: Icon }) => (
          <Tooltip key={id} label={label}>
            <IconButton
              label={label}
              onClick={() => setViewportPreset(id)}
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
          onClick={handleOpenDevtools}
          className="h-6 w-6"
        >
          <Bug className="h-3.5 w-3.5" aria-hidden />
        </IconButton>
      </Tooltip>

      <div ref={menuRootRef} className="relative">
        <IconButton
          label="More browser actions"
          onClick={() => setMenuOpen((v) => !v)}
          className={cn("h-6 w-6", menuOpen && "bg-fill-3 text-ink")}
        >
          <MoreHorizontal className="h-3.5 w-3.5" aria-hidden />
        </IconButton>

        {menuOpen ? (
          <BrowserOverflowMenu
            browserStarted={browserStarted}
            showLiveContent={showLiveContent}
            browserUrl={browserUrl}
            onClose={() => setMenuOpen(false)}
            onScreenshot={handleScreenshot}
            onHardReload={handleHardReload}
            onCopyUrl={handleCopyUrl}
            onClearHistory={handleClearHistory}
            onClearData={handleClearData}
          />
        ) : null}
      </div>
    </div>
  )
}
