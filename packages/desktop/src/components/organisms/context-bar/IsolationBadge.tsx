import { useEffect, useRef, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { GitMerge, XCircle } from "lucide-react"
import { workspaceStatus } from "../../../lib/tauri"
import { useWorkspaceActions } from "../../../hooks/useWorkspaceActions"
import { cn } from "../../../lib/utils"
import { PopoverItem } from "../../molecules/PopoverTray"

/** Isolated-workspace badge → popover with the diff summary + integrate/discard. */
export const IsolationBadge = ({
  sessionId,
  onError,
}: {
  sessionId: string
  onError?: (message: string) => void
}) => {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)

  const workspace = useWorkspaceActions(sessionId, onError, () =>
    setOpen(false),
  )

  const { data: status, isLoading } = useQuery({
    queryKey: ["workspace-status", sessionId],
    queryFn: () => workspaceStatus(sessionId),
    enabled: open,
    staleTime: 2_000,
  })

  useEffect(() => {
    const handlePointer = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener("mousedown", handlePointer)
    return () => document.removeEventListener("mousedown", handlePointer)
  }, [])

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className={cn(
          "ml-1 flex h-6 items-center rounded-full bg-fill-3 px-2 text-xs text-ink-muted",
          "transition-colors hover:bg-fill-2 hover:text-ink-secondary",
          open && "bg-fill-2 text-ink-secondary",
        )}
      >
        Isolated
      </button>

      {open ? (
        <div
          role="dialog"
          className={cn(
            "absolute bottom-full left-0 z-50 mb-1.5 w-64 overflow-hidden rounded-lg",
            "border border-stroke-2 bg-panel shadow-[var(--shadow-md)] animate-tray-in",
          )}
        >
          <div className="border-b border-stroke-3 px-3 py-2">
            <p className="text-sm font-medium text-ink-secondary">
              Isolated workspace
            </p>
            <p className="mt-0.5 text-xs text-ink-muted">
              {isLoading
                ? "Checking changes…"
                : status
                  ? `${status.filesChanged} file${status.filesChanged === 1 ? "" : "s"} changed${status.summary ? ` · ${status.summary}` : ""}`
                  : "No changes yet"}
            </p>
          </div>
          <PopoverItem
            role="menuitem"
            disabled={workspace.busy}
            onClick={() => void workspace.integrate()}
            className="px-3 py-2 text-base text-ink-secondary hover:text-ink"
          >
            <GitMerge className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Integrate into origin
          </PopoverItem>
          <PopoverItem
            role="menuitem"
            disabled={workspace.busy}
            onClick={() => void workspace.discard()}
            className="px-3 py-2 text-base text-ink-secondary hover:text-ink"
          >
            <XCircle className="h-3.5 w-3.5 text-icon-3" aria-hidden />
            Discard workspace
          </PopoverItem>
        </div>
      ) : null}
    </div>
  )
}
