import {
  Camera,
  Copy,
  Eraser,
  History,
  RotateCw,
} from "lucide-react"
import { cn } from "../../../lib/utils"

const menuItemClass = cn(
  "flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-sm",
  "text-ink-secondary transition-colors duration-[var(--duration-fast)] hover:bg-fill-3 hover:text-ink",
  "disabled:pointer-events-none disabled:opacity-40",
)

export type BrowserOverflowMenuProps = {
  browserStarted: boolean
  showLiveContent: boolean
  browserUrl: string
  onClose: () => void
  onScreenshot: () => void | Promise<void>
  onHardReload: () => void
  onCopyUrl: () => void | Promise<void>
  onClearHistory: () => void | Promise<void>
  onClearData: () => void | Promise<void>
}

/** Browser "…" overflow actions menu. */
export const BrowserOverflowMenu = ({
  browserStarted,
  showLiveContent,
  browserUrl,
  onClose,
  onScreenshot,
  onHardReload,
  onCopyUrl,
  onClearHistory,
  onClearData,
}: BrowserOverflowMenuProps) => {
  return (
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
        disabled={!showLiveContent}
        className={menuItemClass}
        onClick={() => {
          onClose()
          void onScreenshot()
        }}
      >
        <Camera className="h-3.5 w-3.5 text-icon-3" aria-hidden />
        Take Screenshot
      </button>
      <button
        type="button"
        role="menuitem"
        disabled={!showLiveContent}
        className={menuItemClass}
        onClick={() => {
          onClose()
          onHardReload()
        }}
      >
        <RotateCw className="h-3.5 w-3.5 text-icon-3" aria-hidden />
        Hard Reload
      </button>
      <button
        type="button"
        role="menuitem"
        disabled={!browserUrl}
        className={menuItemClass}
        onClick={() => {
          onClose()
          void onCopyUrl()
        }}
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
        onClick={() => {
          onClose()
          void onClearHistory()
        }}
      >
        <History className="h-3.5 w-3.5 text-icon-3" aria-hidden />
        Clear Browsing History
      </button>
      <button
        type="button"
        role="menuitem"
        disabled={!showLiveContent}
        className={menuItemClass}
        onClick={() => {
          onClose()
          void onClearData()
        }}
      >
        <Eraser className="h-3.5 w-3.5 text-icon-3" aria-hidden />
        Clear Browsing Data
      </button>
    </div>
  )
}
