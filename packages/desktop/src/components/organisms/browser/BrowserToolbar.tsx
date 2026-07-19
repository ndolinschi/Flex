import {
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
} from "react"
import {
  ArrowLeft,
  ArrowRight,
  Bug,
  Loader2,
  Maximize,
  Monitor,
  MousePointer2,
  RotateCw,
  Smartphone,
  Tablet,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Tooltip } from "../../atoms"
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
  handleClearHistory: () => void | Promise<void>
  handleClearData: () => void | Promise<void>
  browserDesignMode: boolean
  toggleDesignMode: () => void | Promise<void>
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
  browserDesignMode,
  toggleDesignMode,
}: BrowserToolbarProps) => {
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

  return (
    <div
      ref={toolbarRef}
      className="relative z-20 flex h-[var(--header-height)] min-h-[var(--header-height)] shrink-0 items-center gap-1 overflow-hidden border-b border-stroke-3 bg-bg px-2.5"
      data-browser-chrome="toolbar"
    >
      <div className="flex items-center gap-px">
        <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Back" title="Back"
      disabled={!showLiveContent}
      onClick={browserBack}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <ArrowLeft className="h-3.5 w-3.5" aria-hidden />
    </Button>
        <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Forward" title="Forward"
      disabled={!showLiveContent}
      onClick={browserForward}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <ArrowRight className="h-3.5 w-3.5" aria-hidden />
    </Button>
        <div className="relative flex h-6 w-6 items-center justify-center">
          {browserLoading ? (
            <Loader2
              className="h-3.5 w-3.5 animate-spin text-ink-muted"
              aria-hidden
            />
          ) : (
            <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Reload" title="Reload"
      disabled={!showLiveContent}
      onClick={handleReload}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <RotateCw className="h-3.5 w-3.5" aria-hidden />
    </Button>
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
            className="h-6 w-full rounded-sm bg-fill-4 px-2 text-sm text-ink outline-none focus-visible:[box-shadow:0_0_0_1px_var(--color-stroke-2)]"
          />
        ) : (
          <Button
            variant="ghost"
            onClick={() => setEditing(true)}
            className="h-6 w-full justify-start truncate rounded-sm px-2 text-sm cursor-text hover:bg-fill-4"
          >
            {browserStarted ? (
              <FormattedUrl url={browserUrl} />
            ) : (
              <span className="text-ink-muted">Search or enter a URL</span>
            )}
          </Button>
        )}
      </div>

      <div className="flex items-center gap-px">
        {VIEWPORT_PRESETS.map(({ id, label, icon: Icon }) => (
          <Tooltip key={id} label={label}>
            <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={label} title={label}
      onClick={() => setViewportPreset(id)}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
                viewportPreset === id && "bg-surface-muted text-ink",
      )}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden />
    </Button>
          </Tooltip>
        ))}
      </div>

      <Tooltip label={browserDesignMode ? "Exit Design Mode (⌘⇧D)" : "Design Mode (⌘⇧D)"}>
        <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={browserDesignMode ? "Exit Design Mode" : "Design Mode"} title={browserDesignMode ? "Exit Design Mode" : "Design Mode"}
      disabled={!showLiveContent}
      onClick={() => void toggleDesignMode()}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
            browserDesignMode && "bg-surface-muted text-ink",
      )}
    >
      <MousePointer2 className="h-3.5 w-3.5" aria-hidden />
    </Button>
      </Tooltip>

      <Tooltip label="Open DevTools (floating)">
        <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Open DevTools" title="Open DevTools"
      disabled={!showLiveContent}
      onClick={handleOpenDevtools}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <Bug className="h-3.5 w-3.5" aria-hidden />
    </Button>
      </Tooltip>

      <BrowserOverflowMenu
        open={menuOpen}
        onOpenChange={(next) => setMenuOpen(next)}
        browserStarted={browserStarted}
        showLiveContent={showLiveContent}
        browserUrl={browserUrl}
        onScreenshot={handleScreenshot}
        onHardReload={handleHardReload}
        onCopyUrl={handleCopyUrl}
        onClearHistory={handleClearHistory}
        onClearData={handleClearData}
      />
    </div>
  )
}
