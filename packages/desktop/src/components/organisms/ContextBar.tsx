import { useQuery } from "@tanstack/react-query"
import { isIsolated } from "../../lib/tauri"
import { useIsGitRepo } from "../../hooks/useIsGitRepo"
import { cn } from "../../lib/utils"
import { BranchPicker } from "../molecules/BranchPicker"
import { ProjectPicker } from "../molecules/ProjectPicker"
import { UsageRing } from "./context-bar/UsageRing"
import { IsolationBadge } from "./context-bar/IsolationBadge"
import { IsolationPicker } from "./context-bar/IsolationPicker"
import { CommitBar } from "./context-bar/CommitBar"

type ContextBarProps = {
  /** Working directory for tools/git (worktree root when isolated). */
  cwd?: string
  /** Project root for the picker label (`base_cwd ?? cwd`). */
  projectCwd?: string
  sessionId?: string | null
  disabled?: boolean
  onError?: (message: string) => void
  /**
   * Empty New Agent — compact selectors glued to the composer (folder +
   * isolation only). Hides branch / commit / usage so the input reads as one
   * composition unit with Cursor's empty agent strip.
   */
  compact?: boolean
}

/** Context chrome for the composer: empty = compact pills above; active = thin footer below. */
export const ContextBar = ({
  cwd,
  projectCwd,
  sessionId,
  disabled = false,
  onError,
  compact = false,
}: ContextBarProps) => {
  const { data: isolated } = useQuery({
    queryKey: ["is-isolated", sessionId],
    queryFn: () => isIsolated(sessionId!),
    enabled: !!sessionId,
    staleTime: 5_000,
  })

  // Gate the entire git cluster (branch pill + commit bar) on the cwd
  // actually being a git repo — a non-git folder should show none of it
  // rather than a misleading "No branch" pill. `isRepo` defaults to `true`
  // while the query is loading (or has no cwd yet) so the chrome doesn't
  // flash away/in on every session switch; it only ever hides once we
  // positively know there's no repo.
  const { data: isRepo = true } = useIsGitRepo(cwd)

  if (compact) {
    // Bare pill row — no nested card. Cursor's folder|Direct is chrome, not a box.
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

  // Active session: thin footer strip (Cursor branch|Worktree rhythm) — not a
  // second full toolbar. min-height stays for touch targets without looking tall.
  return (
    <div
      className={cn(
        "flex min-h-6 items-center gap-2 px-0.5",
      )}
      data-context-bar="footer"
    >
      {/* min-w-0 + flex-1 (not justify-between) so this group is what shrinks
          under pressure — the gap to the right-hand cluster is a real flex
          gap, not `justify-between`'s leftover space, so it can never
          collapse to 0 and let the two clusters visually collide. */}
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

      <div className="flex shrink-0 items-center gap-2">
        {isRepo && !isolated && sessionId ? (
          <CommitBar sessionId={sessionId} cwd={cwd} onError={onError} />
        ) : null}
        <UsageRing sessionId={sessionId} />
      </div>
    </div>
  )
}
