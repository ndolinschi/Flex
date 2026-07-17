import { useEffect } from "react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { AppMark, TitleBarMenus } from "../molecules/TitleBarMenus"
import {
  CaptionButtons,
  TrafficLights,
} from "../molecules/WindowControls"
import { detectWindowHost, toggleZoomWindow } from "../../lib/windowChrome"
import { cn } from "../../lib/utils"

type WindowTitleBarProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  className?: string
}

/**
 * Cursor-style custom window chrome: traffic lights (macOS) or caption
 * buttons (Windows/Linux), app mark + File/Edit/View/Help, and a drag region.
 * Requires an undecorated Tauri window (`decorations: false`).
 */
export const WindowTitleBar = ({
  onOpenCommandPalette,
  onOpenSearch,
  className,
}: WindowTitleBarProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"

  // Frontend belt-and-suspenders: if window-state (or a stale launch path)
  // left the native frame on, strip it once the title bar mounts.
  useEffect(() => {
    void getCurrentWindow()
      .setDecorations(false)
      .catch(() => undefined)
  }, [])

  return (
    <header
      className={cn(
        "flex h-[var(--titlebar-height)] shrink-0 items-center select-none",
        "border-b border-stroke-3 bg-bg",
        className,
      )}
      role="banner"
      aria-label="Window"
    >
      <div className="flex h-full shrink-0 items-center gap-0.5">
        {isMac ? (
          <div className="flex h-full items-center pl-2.5 pr-0.5">
            <TrafficLights />
          </div>
        ) : (
          <AppMark />
        )}
        <TitleBarMenus
          onOpenCommandPalette={onOpenCommandPalette}
          onOpenSearch={onOpenSearch}
        />
      </div>

      <div
        className="h-full min-w-[48px] flex-1"
        data-tauri-drag-region
        aria-hidden
        // Custom chrome has no native title-bar double-click. Prefer
        // mousedown `detail === 2` because `data-tauri-drag-region` can
        // swallow the second click before `onDoubleClick` fires.
        onMouseDown={(e) => {
          if (e.button === 0 && e.detail === 2) {
            e.preventDefault()
            void toggleZoomWindow()
          }
        }}
        onDoubleClick={() => void toggleZoomWindow()}
      />

      {!isMac ? <CaptionButtons /> : null}
    </header>
  )
}
