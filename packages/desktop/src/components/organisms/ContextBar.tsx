import { useQuery } from "@tanstack/react-query"
import { isIsolated, listSessions } from "../../lib/tauri"
import { SESSIONS_KEY } from "../../hooks/useSessions"
import { useIsGitRepo } from "../../hooks/useIsGitRepo"
import { cn } from "../../lib/utils"
import { BranchPicker } from "../molecules/BranchPicker"
import { ProjectPicker } from "../molecules/ProjectPicker"
import { UsageRing } from "./context-bar/UsageRing"
import { IsolationBadge } from "./context-bar/IsolationBadge"
import { IsolationPicker } from "./context-bar/IsolationPicker"
import { CommitBar } from "./context-bar/CommitBar"

type ContextBarProps = {
  cwd?: string
  projectCwd?: string
  sessionId?: string | null
  disabled?: boolean
  onError?: (message: string) => void
  /** Empty-agent hero: folder + isolation only, above the bubble. */
  compact?: boolean
  /**
   * Tool-pane footer under TabStrip bodies. Same project/branch strip as the
   * composer footer, but without Commit CTA — Changes owns commit chrome, and
   * split view must not show two Commit & Push bars.
   */
  quiet?: boolean
}

export const ContextBar = ({
  cwd,
  projectCwd,
  sessionId,
  disabled = false,
  onError,
  compact = false,
  quiet = false,
}: ContextBarProps) => {
  const { data: isolatedFromApi } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId!),
    enabled: !!sessionId,
    staleTime: 5_000,
  })
  const { data: hasWorkspaceId } = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: listSessions,
    staleTime: 30_000,
    enabled: !!sessionId,
  })
  const isolated =
    isolatedFromApi ??
    (!!sessionId &&
      !!hasWorkspaceId?.find((s) => s.id === sessionId)?.workspace_id)

  const { data: isRepo = true } = useIsGitRepo(cwd)

  if (compact) {
    return (
      <div
        className="inline-flex max-w-full items-center gap-0.5"
        data-context-bar="compact"
      >
        <ProjectPicker
          sessionId={sessionId ?? null}
          cwd={projectCwd || cwd}
          disabled={disabled}
          onError={onError}
        />
        {isolated && sessionId ? (
          <IsolationBadge sessionId={sessionId} onError={onError} />
        ) : null}
        {!isolated && sessionId ? (
          <IsolationPicker
            sessionId={sessionId}
            projectCwd={projectCwd || cwd}
            disabled={disabled}
          />
        ) : null}
      </div>
    )
  }

  return (
    <div
      className={cn(
        "flex min-h-5 items-center gap-1.5 px-0.5",
        quiet && "min-h-5",
      )}
      data-context-bar={quiet ? "pane" : "footer"}
    >
      <div className="flex min-w-0 flex-1 items-center gap-0.5">
        <ProjectPicker
          sessionId={sessionId ?? null}
          cwd={projectCwd || cwd}
          disabled={disabled}
          onError={onError}
        />
        {isRepo ? (
          <BranchPicker cwd={cwd} disabled={disabled} onError={onError} />
        ) : null}
        {isolated && sessionId ? (
          <IsolationBadge sessionId={sessionId} onError={onError} />
        ) : null}
        {!isolated && sessionId ? (
          <IsolationPicker
            sessionId={sessionId}
            projectCwd={projectCwd || cwd}
            disabled={disabled}
          />
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {!quiet && isRepo && !isolated && sessionId ? (
          <CommitBar sessionId={sessionId} cwd={cwd} onError={onError} />
        ) : null}
        <UsageRing sessionId={sessionId} />
      </div>
    </div>
  )
}
