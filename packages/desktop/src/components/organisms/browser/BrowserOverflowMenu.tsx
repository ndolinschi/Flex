import {
  Camera,
  Copy,
  Eraser,
  History,
  RotateCw,
} from "lucide-react"
import { cn } from "../../../lib/utils"
import { Button } from "@/components/ui/button"

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

const menuItemClass =
  "w-full justify-start gap-2 rounded-none px-2.5 py-1.5 text-sm text-ink-secondary hover:bg-fill-3 hover:text-ink"

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
      <Button
        variant="ghost"
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
      </Button>
      <Button
        variant="ghost"
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
      </Button>
      <Button
        variant="ghost"
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
      </Button>
      <div className="mx-2 my-0.5 border-t border-stroke-3" />
      <Button
        variant="ghost"
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
      </Button>
      <Button
        variant="ghost"
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
      </Button>
    </div>
  )
}
