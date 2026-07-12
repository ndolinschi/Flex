import { useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronDown, GitBranch, GitMerge, GitPullRequest } from "lucide-react"
import { Button, TextArea } from "../../atoms"
import { PopoverItem, PopoverTray } from "../../molecules/PopoverTray"
import {
  gitCommitAndPush,
  gitCommitPaths,
  gitCreateBranchAndCommit,
  gitCreatePr,
  toInvokeError,
} from "../../../lib/tauri"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"

type CommitMode = "commit" | "commit-push" | "branch-commit" | "commit-pr"

const MODE_LABEL: Record<CommitMode, string> = {
  commit: "Commit",
  "commit-push": "Commit & Push",
  "branch-commit": "Create Branch & Commit",
  "commit-pr": "Commit & Create PR",
}

/**
 * Commit center footer for the Changes tab (Cursor parity, spec #48):
 * commit-message textarea + a split button (primary action + dropdown of
 * commit variants). Only rendered for non-isolated sessions with at least
 * one file in the (checkbox-driven) selection — isolated sessions use the
 * Keep/Undo integrate flow instead (see `ChangesTab`).
 */
export const CommitCenter = ({
  sessionId,
  selectedPaths,
  totalFiles,
  onError,
}: {
  sessionId: string
  selectedPaths: string[]
  totalFiles: number
  onError: (message: string) => void
}) => {
  const [message, setMessage] = useState("")
  const [busy, setBusy] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [lastMode, setLastMode] = useState<CommitMode>("commit-push")
  const rootRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)

  const trimmed = message.trim()
  const nothingSelected = selectedPaths.length === 0
  const disabled = busy || nothingSelected || !trimmed

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ["git-status"] })
    void queryClient.invalidateQueries({ queryKey: ["git-branch"] })
  }

  const run = async (mode: CommitMode) => {
    if (disabled) return
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
          const outcome = await gitCreatePr(sessionId, trimmed, selectedPaths)
          if (outcome.degradedReason) {
            pushToast(outcome.degradedReason, "error")
          } else {
            pushToast(
              outcome.prUrl ? `PR created: ${outcome.prUrl}` : "PR created",
              "success",
            )
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

  return (
    <div className="flex shrink-0 flex-col gap-2 border-t border-stroke-3 px-3 py-2.5">
      <TextArea
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        placeholder="Commit message"
        aria-label="Commit message"
        autoFocus
        rows={3}
        disabled={busy}
        className="text-sm"
      />
      <div ref={rootRef} className="relative flex items-center">
        <Button
          variant="primary"
          size="sm"
          className="rounded-r-none border-r border-r-accent-hover/40"
          isLoading={busy}
          disabled={disabled}
          onClick={() => void run(lastMode)}
        >
          <GitMerge className="h-3 w-3" aria-hidden />
          {MODE_LABEL[lastMode]}
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
          <ChevronDown className={cn("h-3 w-3", menuOpen && "rotate-180")} aria-hidden />
        </Button>

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
            <li>
              <PopoverItem role="menuitem" onClick={() => void run("commit-push")}>
                <GitMerge className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Commit &amp; Push
              </PopoverItem>
            </li>
            <li>
              <PopoverItem role="menuitem" onClick={() => void run("branch-commit")}>
                <GitBranch className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Create Branch &amp; Commit
              </PopoverItem>
            </li>
            <li>
              <PopoverItem role="menuitem" onClick={() => void run("commit-pr")}>
                <GitPullRequest className="h-3.5 w-3.5 text-icon-3" aria-hidden />
                Commit &amp; Create PR
              </PopoverItem>
            </li>
          </ul>
        </PopoverTray>

        {nothingSelected ? (
          <span className="ml-2 text-xs text-ink-faint">Select files to commit</span>
        ) : null}
      </div>
    </div>
  )
}
