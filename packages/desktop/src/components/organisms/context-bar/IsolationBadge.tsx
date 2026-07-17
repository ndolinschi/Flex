import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { GitMerge, XCircle } from "@/components/icons"
import { workspaceStatus } from "../../../lib/tauri"
import { useWorkspaceActions } from "../../../hooks/useWorkspaceActions"
import { cn } from "../../../lib/utils"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
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

  const workspace = useWorkspaceActions(sessionId, onError, () =>
    setOpen(false),
  )

  const { data: status, isLoading } = useQuery({
    queryKey: ["workspace-status", sessionId],
    queryFn: () => workspaceStatus(sessionId),
    enabled: open,
    staleTime: 2_000,
  })

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-expanded={open}
          className={cn(
            "ml-1 flex h-6 items-center rounded-full bg-fill-3 px-2 text-xs text-ink-muted",
            "transition-colors duration-[var(--duration-fast)] hover:bg-fill-2 hover:text-ink-secondary",
            open && "bg-fill-2 text-ink-secondary",
          )}
        >
          Isolated
        </button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="start"
        sideOffset={6}
        className="w-64 gap-0 overflow-hidden rounded-lg border border-stroke-2 p-0 shadow-[var(--shadow-md)]"
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
      </PopoverContent>
    </Popover>
  )
}
