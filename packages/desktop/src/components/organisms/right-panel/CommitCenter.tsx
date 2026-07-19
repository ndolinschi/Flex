import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { ChevronDown, GitBranch, GitMerge, GitPullRequest } from "lucide-react"
import { Textarea } from "@/components/ui/textarea"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { CreatePrDialog } from "../../molecules/CreatePrDialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  gitCommitAndPush,
  gitCommitPaths,
  gitCreateBranchAndCommit,
  gitCreatePr,
  gitHasRemote,
  toInvokeError,
} from "../../../lib/tauri"
import { toastPrOutcome } from "../../../lib/prOutcomeToast"
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
  const [prDialogOpen, setPrDialogOpen] = useState(false)
  // null = use the remote-aware default until the user picks a mode.
  const [lastMode, setLastMode] = useState<CommitMode | null>(null)
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

  const runCommitPr = async (title: string, body: string) => {
    if (disabled || !hasRemote) return
    setBusy(true)
    setLastMode("commit-pr")
    try {
      const outcome = await gitCreatePr(
        sessionId,
        trimmed,
        selectedPaths,
        title,
        body,
      )
      toastPrOutcome(pushToast, outcome, "Pull request created")
      invalidate()
      setMessage("")
      setPrDialogOpen(false)
    } catch (err) {
      const msg = toInvokeError(err)
      pushToast(`Commit failed: ${msg}`, "error")
      onError(msg)
    } finally {
      setBusy(false)
      setMenuOpen(false)
    }
  }

  const run = async (mode: CommitMode) => {
    if (disabled) return
    if (!hasRemote && PUSH_MODES.has(mode)) return
    if (mode === "commit-pr") {
      setLastMode(mode)
      setMenuOpen(false)
      setPrDialogOpen(true)
      return
    }
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
    <>
      <div className="flex shrink-0 flex-col gap-2 border-t border-stroke-3 bg-fill-5/40 px-2.5 py-2.5">
        <Textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="Commit message"
          aria-label="Commit message"
          autoFocus
          rows={2}
          disabled={busy}
          className="min-h-[2.5rem] resize-none rounded-[var(--radius-md)] text-sm"
        />
        <div className="relative flex items-center justify-between gap-2">
          <span
            className={cn(
              "min-w-0 truncate text-xs",
              nothingSelected ? "text-ink-faint" : "text-ink-muted",
            )}
          >
            {selectionLabel}
          </span>
          <div className="flex shrink-0 items-center overflow-hidden rounded-md">
            <Button
              variant="default"
              size="sm"
              className="rounded-none border-r border-r-accent-hover/40"
              disabled={busy || disabled}
              onClick={() => void run(effectivePrimary)}
            >
              {busy ? <Spinner data-icon="inline-start" /> : null}
              <GitMerge className="h-3 w-3" aria-hidden />
              {MODE_LABEL[effectivePrimary]}
            </Button>
            <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
              <DropdownMenuTrigger
                disabled={disabled}
                render={
                  <Button
                    type="button"
                    variant="default"
                    size="sm"
                    disabled={disabled}
                    aria-label="Commit options"
                    className="w-7 rounded-none px-0"
                  />
                }
              >
                <ChevronDown
                  className={cn("size-3", menuOpen && "rotate-180")}
                  aria-hidden
                />
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" side="top" sideOffset={6} className="w-56">
                <DropdownMenuGroup>
                  <DropdownMenuItem onClick={() => void run("commit")}>
                    <GitMerge />
                    Commit
                  </DropdownMenuItem>
                  {hasRemote ? (
                    <DropdownMenuItem onClick={() => void run("commit-push")}>
                      <GitMerge />
                      Commit &amp; Push
                    </DropdownMenuItem>
                  ) : null}
                  <DropdownMenuItem onClick={() => void run("branch-commit")}>
                    <GitBranch />
                    Create Branch &amp; Commit
                  </DropdownMenuItem>
                  {hasRemote ? (
                    <DropdownMenuItem onClick={() => void run("commit-pr")}>
                      <GitPullRequest />
                      Commit &amp; Create PR
                    </DropdownMenuItem>
                  ) : null}
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </div>

      <CreatePrDialog
        open={prDialogOpen}
        initialTitle={trimmed}
        initialBody=""
        isLoading={busy}
        onCancel={() => {
          if (!busy) setPrDialogOpen(false)
        }}
        onConfirm={(title, body) => {
          void runCommitPr(title, body)
        }}
      />
    </>
  )
}
