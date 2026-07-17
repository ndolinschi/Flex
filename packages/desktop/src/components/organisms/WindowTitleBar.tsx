import { useEffect, useState } from "react"
import { PanelLeft } from "lucide-react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { AppMark, TitleBarMenus } from "../molecules/TitleBarMenus"
import { BugReportDialog } from "../molecules/BugReportDialog"
import {
  CaptionButtons,
  TrafficLights,
} from "../molecules/WindowControls"
import { IconButton } from "../atoms"
import { useNativeAppMenu } from "../../hooks/useNativeAppMenu"
import { useTitleBarActions } from "../../hooks/useTitleBarActions"
import { detectWindowHost, toggleZoomWindow } from "../../lib/windowChrome"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

type WindowTitleBarProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  className?: string
}

/**
 * Custom window chrome: traffic lights (macOS) or caption buttons
 * (Windows/Linux), optional in-window File/Edit/View/Help (non-macOS),
 * sidebar toggle, and a drag region. macOS uses the native menu bar instead.
 * Requires an undecorated Tauri window (`decorations: false`).
 */
export const WindowTitleBar = ({
  onOpenCommandPalette,
  onOpenSearch,
  className,
}: WindowTitleBarProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"
  const [bugOpen, setBugOpen] = useState(false)
  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)

  const { handlers } = useTitleBarActions({
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport: () => setBugOpen(true),
  })

  useNativeAppMenu({
    enabled: isMac,
    isBootstrapped,
    canSearch: Boolean(onOpenSearch),
    canCommandPalette: Boolean(onOpenCommandPalette),
    handlers,
  })

  // Frontend belt-and-suspenders: if window-state (or a stale launch path)
  // left the native frame on, strip it once the title bar mounts.
  useEffect(() => {
    void getCurrentWindow()
      .setDecorations(false)
      .catch(() => undefined)
  }, [])

  const mod = isMac ? "⌘" : "Ctrl+"
  const sidebarToggle = (
    <IconButton
      label={`${collapsed ? "Show" : "Hide"} sidebar (${mod}B)`}
      onClick={handlers.toggleSidebar}
      disabled={!isBootstrapped}
      quiet
      className="h-6 w-6 shrink-0"
    >
      <PanelLeft className="h-3.5 w-3.5" aria-hidden />
    </IconButton>
  )

  return (
    <>
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
          {!isMac ? (
            <TitleBarMenus
              handlers={handlers}
              isBootstrapped={isBootstrapped}
              canSearch={Boolean(onOpenSearch)}
              canCommandPalette={Boolean(onOpenCommandPalette)}
            />
          ) : null}
          {sidebarToggle}
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
      <BugReportDialog open={bugOpen} onClose={() => setBugOpen(false)} />
    </>
  )
}
