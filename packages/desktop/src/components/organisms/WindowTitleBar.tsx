import { memo, useCallback, useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import { Columns2, PanelLeft } from "lucide-react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { AppMark, TitleBarMenus } from "../molecules/TitleBarMenus"
import { BugReportDialog } from "../molecules/BugReportDialog"
import { SessionMenu } from "../molecules/SessionMenu"
import {
  CaptionButtons,
  TrafficLights,
} from "../molecules/WindowControls"
import { useNativeAppMenu } from "../../hooks/useNativeAppMenu"
import { useTitleBarActions } from "../../hooks/useTitleBarActions"
import { useSessions } from "../../hooks/useSessions"
import { detectWindowHost, toggleZoomWindow } from "../../lib/windowChrome"
import { sessionLabel } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { isSplitEligible } from "../../stores/slices/contentLayoutSlice"
import { cn } from "../../lib/utils"

type WindowTitleBarProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  className?: string
}

/**
 * Custom window chrome: traffic lights (macOS) or caption buttons
 * (Windows/Linux), optional in-window File/Edit/View/Help (non-macOS),
 * sidebar / split / session controls, and a drag region. macOS uses the
 * native menu bar instead of in-window menus.
 * Requires an undecorated Tauri window (`decorations: false`).
 */
const WindowTitleBarImpl = ({
  onOpenCommandPalette,
  onOpenSearch,
  className,
}: WindowTitleBarProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"
  const [bugOpen, setBugOpen] = useState(false)
  const openBugReport = useCallback(() => setBugOpen(true), [])
  const closeBugReport = useCallback(() => setBugOpen(false), [])

  const collapsed = useAppStore((s) => s.sidebarCollapsed)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const route = useAppStore((s) => s.route)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  // Narrow — full `contentLayout` changes on every tab switch.
  const split = useAppStore((s) => s.contentLayout.mode === "split")
  const splitEligible = useAppStore(isSplitEligible)
  const toggleSplit = useAppStore((s) => s.toggleSplit)
  const viewport = useAppStore((s) => s.viewport)

  const { sessions, newAgent, renameSession, deleteSession } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const title = active ? sessionLabel(active) : "Agent"
  const showChatChrome = isBootstrapped && route !== "welcome"

  const { handlers } = useTitleBarActions({
    newAgent,
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport: openBugReport,
  })

  useNativeAppMenu({
    enabled: isMac,
    isBootstrapped,
    canSearch: Boolean(onOpenSearch),
    canCommandPalette: Boolean(onOpenCommandPalette),
    handlers,
  })

  useEffect(() => {
    void getCurrentWindow()
      .setDecorations(false)
      .catch(() => undefined)
  }, [])

  const mod = isMac ? "⌘" : "Ctrl+"

  return (
    <>
      <header
        className={cn(
          // Compact density: 30px chrome (`--titlebar-height`); h-6 controls only.
          "flex h-[var(--titlebar-height)] shrink-0 items-center select-none",
          // Continuous surface: paint-free over `.app-shell` / macOS HudWindow
          // vibrancy so glass reads through; hairline stroke-3 only.
          "border-b border-stroke-3 bg-transparent",
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
          {showChatChrome ? (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              aria-label={`${collapsed ? "Show" : "Hide"} sidebar (${mod}B)`}
              title={`${collapsed ? "Show" : "Hide"} sidebar (${mod}B)`}
              onClick={handlers.toggleSidebar}
              className={cn(
                "text-ink-muted hover:bg-fill-4 hover:text-ink",
                "opacity-50 hover:opacity-80",
                "shrink-0",
              )}
            >
              <PanelLeft className="h-3.5 w-3.5" aria-hidden />
            </Button>
          ) : null}
        </div>

        <div
          className="h-full min-w-[48px] flex-1"
          data-tauri-drag-region
          aria-hidden
          onDoubleClick={() => void toggleZoomWindow()}
        />

        {showChatChrome ? (
          <div className="flex h-full shrink-0 items-center gap-0.5 pr-1">
            {viewport === "wide" && (split || splitEligible) ? (
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                aria-label={`${split ? "Close split" : "Split view"} (${mod}J)`}
                title={`${split ? "Close split" : "Split view"} (${mod}J)`}
                onClick={toggleSplit}
                disabled={!split && !splitEligible}
                aria-pressed={split}
                className={cn(
                  // Quiet title-bar icon recipe; pressed = split active (fill-2, full ink).
                  "text-ink-muted opacity-50 hover:bg-fill-4 hover:text-ink hover:opacity-80",
                  split &&
                    "bg-fill-2 text-ink opacity-80 hover:bg-fill-2 hover:opacity-100",
                )}
              >
                <Columns2 className="h-3.5 w-3.5" aria-hidden />
              </Button>
            ) : null}
            {active ? (
              <SessionMenu
                sessionId={active.id}
                label={title}
                onRename={renameSession}
                onDelete={deleteSession}
              />
            ) : null}
          </div>
        ) : null}

        {!isMac ? <CaptionButtons /> : null}
      </header>
      <BugReportDialog open={bugOpen} onClose={closeBugReport} />
    </>
  )
}

export const WindowTitleBar = memo(WindowTitleBarImpl)
