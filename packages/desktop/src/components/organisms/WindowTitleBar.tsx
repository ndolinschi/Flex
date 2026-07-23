import { memo, useCallback, useEffect, useState } from "react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { TitleBarMenus } from "../molecules/TitleBarMenus"
import { BugReportDialog } from "../molecules/BugReportDialog"
import {
  CaptionButtons,
  TrafficLights,
} from "../molecules/WindowControls"
import { useNativeAppMenu } from "../../hooks/useNativeAppMenu"
import { useTitleBarActions } from "../../hooks/useTitleBarActions"
import { useSessions } from "../../hooks/useSessions"
import { detectWindowHost } from "../../lib/windowChrome"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { TitleBarDragRegion } from "./TitleBarChrome"

type WindowTitleBarProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  className?: string
}

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

  return (
    <>
      <header
        className={cn(
          "glass-titleband flex h-[var(--titlebar-height)] shrink-0 items-center select-none",
          "border-b border-stroke-3",
          className,
        )}
        role="banner"
        aria-label="Window"
        data-component="glass-titlebar"
        data-slot="glass-titleband"
      >
        <div className="flex h-full shrink-0 items-center gap-0.5">
          {isMac ? (
            <div className="flex h-full items-center pl-2 pr-0.5">
              <TrafficLights />
            </div>
          ) : null}
          {!isMac ? (
            <TitleBarMenus
              handlers={handlers}
              isBootstrapped={isBootstrapped}
              canSearch={Boolean(onOpenSearch)}
              canCommandPalette={Boolean(onOpenCommandPalette)}
            />
          ) : null}
        </div>

        <TitleBarDragRegion />

        {!isMac ? <CaptionButtons /> : null}
      </header>
      <BugReportDialog open={bugOpen} onClose={closeBugReport} />
    </>
  )
}

export const WindowTitleBar = memo(WindowTitleBarImpl)
