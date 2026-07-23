import { memo, useCallback, useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import { Columns2, PanelLeft } from "lucide-react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { TitleBarMenus } from "../molecules/TitleBarMenus"
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

type TitleBarChromeHostProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
}

export const TitleBarChromeHost = ({
  onOpenCommandPalette,
  onOpenSearch,
}: TitleBarChromeHostProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"
  const [bugOpen, setBugOpen] = useState(false)
  const openBugReport = useCallback(() => setBugOpen(true), [])
  const closeBugReport = useCallback(() => setBugOpen(false), [])

  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const { newAgent } = useSessions()

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

  return <BugReportDialog open={bugOpen} onClose={closeBugReport} />
}

type TitleBarLeadingProps = {
  showWindowControls?: boolean
  showSidebarReopen?: boolean
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  className?: string
}

export const TitleBarLeading = memo(({
  showWindowControls = false,
  showSidebarReopen = false,
  onOpenCommandPalette,
  onOpenSearch,
  className,
}: TitleBarLeadingProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)
  const setSidebarCollapsed = useAppStore((s) => s.setSidebarCollapsed)
  const { newAgent } = useSessions()
  const [bugOpen, setBugOpen] = useState(false)
  const openBugReport = useCallback(() => setBugOpen(true), [])
  const closeBugReport = useCallback(() => setBugOpen(false), [])

  const { handlers } = useTitleBarActions({
    newAgent,
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport: openBugReport,
  })

  const mod = isMac ? "⌘" : "Ctrl+"

  if (!showWindowControls && !showSidebarReopen) return null

  return (
    <>
      <div className={cn("flex h-full shrink-0 items-center gap-0.5", className)}>
        {showWindowControls ? (
          isMac ? (
            <div className="flex h-full items-center pl-2 pr-0.5">
              <TrafficLights />
            </div>
          ) : (
            <>
              <TitleBarMenus
                handlers={handlers}
                isBootstrapped={isBootstrapped}
                canSearch={Boolean(onOpenSearch)}
                canCommandPalette={Boolean(onOpenCommandPalette)}
              />
            </>
          )
        ) : null}
        {showSidebarReopen ? (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={`Show sidebar (${mod}B)`}
            title={`Show sidebar (${mod}B)`}
            onClick={() => setSidebarCollapsed(false)}
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
      {!isMac && showWindowControls ? (
        <BugReportDialog open={bugOpen} onClose={closeBugReport} />
      ) : null}
    </>
  )
})
TitleBarLeading.displayName = "TitleBarLeading"

type TitleBarTrailingProps = {
  showChatActions?: boolean
  className?: string
}

export const TitleBarTrailing = memo(({
  showChatActions = true,
  className,
}: TitleBarTrailingProps) => {
  const host = detectWindowHost()
  const isMac = host === "macos"
  const split = useAppStore((s) => s.contentLayout.mode === "split")
  const splitEligible = useAppStore(isSplitEligible)
  const toggleSplit = useAppStore((s) => s.toggleSplit)
  const viewport = useAppStore((s) => s.viewport)
  const activeSessionId = useAppStore((s) => s.activeSessionId)
  const { sessions, renameSession, deleteSession } = useSessions()
  const active = sessions.find((s) => s.id === activeSessionId)
  const title = active ? sessionLabel(active) : "Agent"
  const mod = isMac ? "⌘" : "Ctrl+"

  return (
    <div className={cn("flex h-full shrink-0 items-center gap-0.5", className)}>
      {showChatActions ? (
        <>
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
        </>
      ) : null}
      {!isMac ? <CaptionButtons /> : null}
    </div>
  )
})
TitleBarTrailing.displayName = "TitleBarTrailing"

export const TitleBarDragRegion = ({ className }: { className?: string }) => (
  <div
    className={cn("h-full min-w-[24px] flex-1", className)}
    data-tauri-drag-region
    aria-hidden
    onDoubleClick={() => void toggleZoomWindow()}
  />
)
