import { useEffect, useState, type ReactNode } from "react"
import {
  closeWindow,
  detectWindowHost,
  isWindowMaximized,
  minimizeWindow,
  toggleZoomWindow,
  type WindowHost,
} from "../../lib/windowChrome"
import { cn } from "../../lib/utils"

type WindowControlsProps = {
  host?: WindowHost
  className?: string
}

const TrafficButton = ({
  label,
  tone,
  onClick,
  icon,
}: {
  label: string
  tone: "close" | "minimize" | "zoom"
  onClick: () => void
  icon: ReactNode
}) => (
  <button
    type="button"
    aria-label={label}
    title={label}
    onClick={onClick}
    className={cn(
      "group relative flex h-3 w-3 items-center justify-center rounded-full",
      "outline-none transition-opacity",
      tone === "close" && "bg-[#ff5f57] hover:brightness-95",
      tone === "minimize" && "bg-[#febc2e] hover:brightness-95",
      tone === "zoom" && "bg-[#28c840] hover:brightness-95",
    )}
  >
    <span className="pointer-events-none absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100">
      {icon}
    </span>
  </button>
)

/** macOS-style traffic lights (close / minimize / zoom). */
export const TrafficLights = ({ className }: { className?: string }) => (
  <div className={cn("flex items-center gap-[6px] px-1", className)}>
    <TrafficButton
      label="Close"
      tone="close"
      onClick={() => void closeWindow()}
      icon={
        <svg width="6" height="6" viewBox="0 0 6 6" aria-hidden>
          <path
            d="M1 1l4 4M5 1L1 5"
            stroke="#4d0000"
            strokeWidth="1.2"
            strokeLinecap="round"
          />
        </svg>
      }
    />
    <TrafficButton
      label="Minimize"
      tone="minimize"
      onClick={() => void minimizeWindow()}
      icon={
        <svg width="6" height="6" viewBox="0 0 6 6" aria-hidden>
          <path
            d="M1 3h4"
            stroke="#995700"
            strokeWidth="1.2"
            strokeLinecap="round"
          />
        </svg>
      }
    />
    <TrafficButton
      label="Full Screen"
      tone="zoom"
      onClick={() => void toggleZoomWindow()}
      icon={
        <svg width="6" height="6" viewBox="0 0 6 6" aria-hidden>
          <path
            d="M1.5 4.5h3v-3"
            fill="none"
            stroke="#006500"
            strokeWidth="1.1"
            strokeLinejoin="round"
          />
        </svg>
      }
    />
  </div>
)

/** Windows/Linux caption buttons (minimize / maximize / close). */
export const CaptionButtons = ({ className }: { className?: string }) => {
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined
    const win = (() => {
      try {
        return import("@tauri-apps/api/window").then((m) => m.getCurrentWindow())
      } catch {
        return Promise.resolve(null)
      }
    })()

    void (async () => {
      const appWindow = await win
      if (!appWindow || cancelled) return
      try {
        setMaximized(await appWindow.isMaximized())
      } catch {
        /* browser / missing permission */
      }
      try {
        unlisten = await appWindow.onResized(async () => {
          if (cancelled) return
          try {
            setMaximized(await appWindow.isMaximized())
          } catch {
            /* ignore */
          }
        })
      } catch {
        /* ignore */
      }
    })()

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  return (
    <div className={cn("flex h-full shrink-0 items-stretch", className)}>
      <button
        type="button"
        aria-label="Minimize"
        title="Minimize"
        onClick={() => void minimizeWindow()}
        className="flex h-full w-10 items-center justify-center text-ink-secondary transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <path d="M1 5h8" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      <button
        type="button"
        aria-label={maximized ? "Restore" : "Maximize"}
        title={maximized ? "Restore" : "Maximize"}
        onClick={() => void toggleZoomWindow().then(() => isWindowMaximized().then(setMaximized))}
        className="flex h-full w-10 items-center justify-center text-ink-secondary transition-colors duration-[var(--duration-fast)] hover:bg-fill-4 hover:text-ink"
      >
        {maximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path
              d="M2.5 3.5h5v5h-5zM3.5 2.5h5v5"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <rect
              x="1.5"
              y="1.5"
              width="7"
              height="7"
              fill="none"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        )}
      </button>
      <button
        type="button"
        aria-label="Close"
        title="Close"
        onClick={() => void closeWindow()}
        className="flex h-full w-10 items-center justify-center text-ink-secondary transition-colors duration-[var(--duration-fast)] hover:bg-[#c42b1c] hover:text-white"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <path
            d="M2 2l6 6M8 2L2 8"
            stroke="currentColor"
            strokeWidth="1.1"
            strokeLinecap="round"
          />
        </svg>
      </button>
    </div>
  )
}

/** Platform-aware window controls — traffic lights on macOS, caption buttons elsewhere. */
export const WindowControls = ({
  host = detectWindowHost(),
  className,
}: WindowControlsProps) => {
  if (host === "macos") {
    return <TrafficLights className={className} />
  }
  return <CaptionButtons className={className} />
}
