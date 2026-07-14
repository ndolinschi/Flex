import { useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronDown, GitBranch, GitMerge, GitPullRequest } from "lucide-react"
import { Button, TextArea } from "../../atoms"
import { PopoverItem, PopoverTray } from "../../molecules/PopoverTray"
import {
  gitCommitAndPush,
  gitCommitPaths,
  gitCreateBranchAndCommit,
  gitCreatePr,
  gitHasRemote,
  toInvokeError,
} from "../../../lib/tauri"
import { openExternalUrl } from "../../../lib/openExternalUrl"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"

type CommitMode = "commit" | "commit-push" | "branch-commit" | "commit-pr"

const MODE_LABEL: Record<CommitMode, string> = {
  commit: "Commit",
  "commit-push": "Commit & Push",
  "branch-commit": "Create Branch & Commit",
  "commit-pr": "Commit & Create PR",
}

const PUSH_MODES: ReadonlySet<CommitMode> = new Set(["commit-push", "commit-pr"])

/**
 * Commit center footer for the Changes tab (Cursor parity, spec #48):
 * commit-message textarea + a split button (primary action + dropdown of
 * commit variants). Only rendered for non-isolated sessions with at least
 * one file in the (checkbox-driven) selection — isolated sessions use the
 * Keep/Undo integrate flow instead (see `ChangesTab`).
 *
 * Push / PR modes are offered only when the session cwd has a configured
 * remote — otherwise the primary action is plain Commit (push would fail
 * with "No configured push destination").
 */
export const CommitCenter = ({
  sessionId,
  cwd,
  selectedPaths,
  totalFiles,
  onError,
}: {
  sessionId: string
  cwd: string
  selectedPaths: string[]
  totalFiles: number
  onError: (message: string) => void
}) => {
  const [message, setMessage] = useState("")
  const [busy, setBusy] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  // null = use the remote-aware default until the user picks a mode.
  const [lastMode, setLastMode] = useState<CommitMode | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)

  const { data: hasRemote = false } = useQuery({
    queryKey: ["git-has-remote", cwd],
    queryFn: () => gitHasRemote(cwd),
    enabled: !!cwd,
    staleTime: 10_000,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
  })

  const defaultMode: CommitMode = hasRemote ? "commit-push" : "commit"
  const effectivePrimary: CommitMode =
    lastMode && (hasRemote || !PUSH_MODES.has(lastMode))
      ? lastMode
      : defaultMode

  const trimmed = message.trim()
  const nothingSelected = selectedPaths.length === 0
  const disabled = busy || nothingSelected || !trimmed

  const invalidate = () => {
    invalidateGitQueries(queryClient)
    void queryClient.invalidateQueries({ queryKey: ["git-branch"] })
  }

  const run = async (mode: CommitMode) => {
    if (disabled) return
    if (!hasRemote && PUSH_MODES.has(mode)) return
    setBusy(true)
    setLastMode(mode)
    try {
      switch (mode) {
        case "commit": {
          const sha = await gitCommitPaths(sessionId, trimmed, selectedPaths)
          pushToast(`Committed ${sha}`, "success")
          break
        }
        case "commit-push": {
          const sha = await gitCommitAndPush(sessionId, trimmed, selectedPaths)
          pushToast(`Committed ${sha} and pushed`, "success")
          break
        }
        case "branch-commit": {
          const branch = window.prompt("New branch name")?.trim()
          if (!branch) {
            setBusy(false)
            return
          }
          const sha = await gitCreateBranchAndCommit(
            sessionId,
            branch,
            trimmed,
            selectedPaths,
          )
          pushToast(`Committed ${sha} on ${branch}`, "success")
          break
        }
        case "commit-pr": {
          const outcome = await gitCreatePr(
            sessionId,
            trimmed,
            selectedPaths,
            trimmed,
          )
          if (outcome.degradedReason) {
            // Commit+push still landed — non-fatal for the user.
            pushToast(outcome.degradedReason, "success")
          } else if (outcome.prUrl) {
            const url = outcome.prUrl
            pushToast("Pull request created", "success", {
              label: "Open PR",
              onAction: () => {
                void openExternalUrl(url)
              },
            })
          } else {
            pushToast("Pull request created", "success")
          }
          break
        }
      }
      invalidate()
      setMessage("")
    } catch (err) {
      const msg = toInvokeError(err)
      pushToast(`Commit failed: ${msg}`, "error")
      onError(msg)
    } finally {
      setBusy(false)
      setMenuOpen(false)
    }
  }

  if (totalFiles === 0) return null

  const selectionLabel = nothingSelected
    ? "Select files to commit"
    : `${selectedPaths.length} file${selectedPaths.length === 1 ? "" : "s"}`

  return (
    <div className="flex shrink-0 flex-col gap-2.5 border-t border-stroke-3 bg-fill-5/40 px-3 py-3">
      <TextArea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        placeholder="Commit message"
        aria-label="Commit message"
        autoFocus
        rows={2}
        disabled={busy}
        className="min-h-[2.75rem] resize-none text-sm"
      />
      <div ref={rootRef} className="relative flex items-center justify-between gap-3">
        <span
          className={cn(
            "min-w-0 truncate text-xs",
            nothingSelected ? "text-ink-faint" : "text-ink-muted",
          )}
        >
          {selectionLabel}
        </span>
        <div className="flex shrink-0 items-center">
          <Button
            variant="primary"
            size="sm"
            className="rounded-r-none border-r border-r-accent-hover/40"
            isLoading={busy}
            disabled={disabled}
            onClick={() => void run(effectivePrimary)}
          >
            <GitMerge className="h-3 w-3" aria-hidden />
            {MODE_LABEL[effectivePrimary]}
          </Button>
          <Button
            variant="primary"
            size="sm"
            aria-label="Commit options"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            className="w-7 rounded-l-none px-0"
            disabled={disabled}
            onClick={() => setMenuOpen((v) => !v)}
          >
            <ChevronDown
              className={cn("h-3 w-3", menuOpen && "rotate-180")}
              aria-hidden
            />
          </Button>
        </div>

        <PopoverTray
          open={menuOpen}
          onClose={() => setMenuOpen(false)}
          anchorRef={rootRef}
          placement="above"
          role="menu"
          aria-label="Commit options"
          className="right-0 w-56"
        >
          <ul className="py-1">
            <li>
              <PopoverItem role="menuitem" onClick={() => void run("commit")}>
                <GitMerge className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Commit
              </PopoverItem>
            </li>
            {hasRemote ? (
              <li>
                <PopoverItem
                  role="menuitem"
                  onClick={() => void run("commit-push")}
                >
                  <GitMerge className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                  Commit &amp; Push
                </PopoverItem>
              </li>
            ) : null}
            <li>
              <PopoverItem
                role="menuitem"
                onClick={() => void run("branch-commit")}
              >
                <GitBranch className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Create Branch &amp; Commit
              </PopoverItem>
            </li>
            {hasRemote ? (
              <li>
                <PopoverItem
                  role="menuitem"
                  onClick={() => void run("commit-pr")}
                >
                  <GitPullRequest
                    className="h-3.5 w-3.5 text-icon-3"
                    aria-hidden
                  />
                  Commit &amp; Create PR
                </PopoverItem>
              </li>
            ) : null}
          </ul>
        </PopoverTray>
      </div>
    </div>
  )
}
