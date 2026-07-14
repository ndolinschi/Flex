import { AppMark, TitleBarMenus } from "../molecules/TitleBarMenus"
import {
  CaptionButtons,
  TrafficLights,
} from "../molecules/WindowControls"
import { detectWindowHost } from "../../lib/windowChrome"
import { cn } from "../../lib/utils"

type WindowTitleBarProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  className?: string
}

/**
 * Cursor-style custom window chrome: traffic lights (macOS) or caption
 * buttons (Windows/Linux), app mark + File/Edit/View/Help, and a drag region.
 * Requires `decorations: false` on the Tauri window.
 */
export const WindowTitleBar = ({
  onOpenCommandPalette,
  onOpenSearch,
  className,
}: WindowTitleBarProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"

  return (
    <header
      className={cn(
        "flex h-[var(--titlebar-height)] shrink-0 items-stretch select-none",
        "border-b border-stroke-3 bg-bg",
        className,
      )}
      role="banner"
      aria-label="Window"
    >
      <div className="flex shrink-0 items-center">
        {isMac ? (
          <div className="flex h-full items-center pl-3 pr-1">
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
      />

      {!isMac ? <CaptionButtons /> : null}
    </header>
  )
}
