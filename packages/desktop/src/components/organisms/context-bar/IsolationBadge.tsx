import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { GitMerge, XCircle } from "lucide-react"
import { isSessionNotFoundError } from "../../../lib/sessions"
import { toInvokeError, workspaceStatus } from "../../../lib/tauri"
import { useWorkspaceActions } from "../../../hooks/useWorkspaceActions"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

/** Isolated-workspace badge → menu with the diff summary + integrate/discard. */
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
    queryFn: async () => {
      try {
        return await workspaceStatus(sessionId)
      } catch (err) {
        const message = toInvokeError(err)
        if (isSessionNotFoundError(message)) return null
        throw err
      }
    },
    enabled: open,
    staleTime: 2_000,
    retry: false,
  })

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            className="ml-1 h-6 rounded-full bg-transparent px-2 text-xs font-normal text-ink-muted hover:bg-fill-4 hover:text-ink aria-expanded:bg-fill-4 aria-expanded:text-ink"
          />
        }
      >
        Isolated
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side="top" sideOffset={6} className="w-64">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="font-medium text-ink">
            Isolated workspace
          </DropdownMenuLabel>
          <div className="px-1.5 pb-1.5 text-xs text-ink-muted">
            {isLoading
              ? "Checking changes…"
              : status
                ? `${status.filesChanged} file${status.filesChanged === 1 ? "" : "s"} changed${status.summary ? ` · ${status.summary}` : ""}`
                : "No changes yet"}
          </div>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem
            disabled={workspace.busy}
            onClick={() => void workspace.integrate()}
          >
            <GitMerge />
            Integrate into origin
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={workspace.busy}
            onClick={() => void workspace.discard()}
          >
            <XCircle />
            Discard workspace
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
