import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitMerge, GitPullRequest } from "lucide-react"
import {
  gitCommit,
  gitCreatePrForBranch,
  gitHasRemote,
  gitPush,
  gitStatusSinceBaseline,
  toInvokeError,
} from "../../../lib/tauri"
import { toastPrOutcome } from "../../../lib/prOutcomeToast"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import { sessionHasActivity, useAppStore } from "../../../stores/appStore"
import { CreatePrDialog } from "../../molecules/CreatePrDialog"
import { DiffStat } from "../../atoms"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Spinner } from "@/components/ui/spinner"
import { Input } from "@/components/ui/input"

/** Right-aligned "N changes" pill + Commit button, shown above the
 * composer for non-isolated sessions with a dirty working tree (design:
 * "Changes +9745 -737" pill + button). Clicking the pill jumps to the
 * Changes tab; the button opens an inline popover to compose the message.
 *
 * Label / actions depend on remotes: no remote → Commit only; with a
 * remote → Commit, Commit & Push, and Commit & Create PR. */
export const CommitBar = ({
  sessionId,
  cwd,
  onError,
}: {
  sessionId: string
  cwd?: string
  onError?: (message: string) => void
}) => {
  const [open, setOpen] = useState(false)
  const [prDialogOpen, setPrDialogOpen] = useState(false)
  const [message, setMessage] = useState("Update from agent session")
  const [busy, setBusy] = useState<"commit" | "push" | "pr" | null>(null)
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  const openToolBesideChat = useAppStore((s) => s.openToolBesideChat)
  // Hide pre-turn repo dirt on a brand-new chat (same gate as FilesChangedCard).
  const hasActivity = useAppStore((s) => sessionHasActivity(s, sessionId))

  const { data: summary } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId),
    enabled: !!cwd && !!sessionId && hasActivity,
    staleTime: 5_000,
  })

  const { data: hasRemote = false } = useQuery({
    queryKey: ["git-has-remote", cwd ?? ""],
    queryFn: () => gitHasRemote(cwd!),
    enabled: !!cwd,
    staleTime: 10_000,
    refetchOnMount: "always",
    refetchOnWindowFocus: true,
  })

  // This pill only ever shows a count + aggregate +/- badge (no file rows),
  // so it reads straight from the summary's totals — always accurate even
  // past the server-side row cap.
  const totalCount = summary?.totalCount ?? 0
  const totals = {
    added: summary?.totalAdded ?? 0,
    removed: summary?.totalRemoved ?? 0,
  }

  const primaryLabel = hasRemote ? "Commit & Push" : "Commit"
  const trimmed = message.trim()

  const handleCommit = async (andPush: boolean) => {
    if (busy) return
    if (andPush && !hasRemote) return
    setBusy("commit")
    try {
      // TODO: gitCommit stages the whole repo (`git add -A` in the Rust
      // `git_commit` command) even though the count/list above is
      // session-scoped (gitStatusSinceBaseline). A session with 0 tracked
      // changes can still commit unrelated pre-existing dirty files repo-wide.
      const sha = await gitCommit(sessionId, trimmed)
      invalidateGitQueries(queryClient)
      pushToast(`Committed ${sha}`, "success")
      if (andPush) {
        setBusy("push")
        try {
          await gitPush(sessionId)
          pushToast("Pushed", "success")
        } catch (err) {
          const msg = toInvokeError(err)
          pushToast(`Push failed: ${msg}`, "error")
          onError?.(msg)
        }
      }
      setOpen(false)
    } catch (err) {
      const msg = toInvokeError(err)
      pushToast(`Commit failed: ${msg}`, "error")
      onError?.(msg)
    } finally {
      setBusy(null)
    }
  }

  const handleCommitPr = async (title: string, body: string) => {
    if (busy || !cwd || !hasRemote) return
    setBusy("pr")
    try {
      const sha = await gitCommit(sessionId, trimmed)
      invalidateGitQueries(queryClient)
      try {
        await gitPush(sessionId)
      } catch (err) {
        const msg = toInvokeError(err)
        pushToast(`Committed ${sha}, but push failed: ${msg}`, "error")
        onError?.(msg)
        return
      }
      const outcome = await gitCreatePrForBranch(cwd, title, body)
      invalidateGitQueries(queryClient)
      toastPrOutcome(pushToast, outcome, "Pull request created")
      setPrDialogOpen(false)
      setOpen(false)
    } catch (err) {
      const msg = toInvokeError(err)
      pushToast(`Commit failed: ${msg}`, "error")
      onError?.(msg)
    } finally {
      setBusy(null)
    }
  }

  if (totalCount === 0) return null

  return (
    <div className="relative flex shrink-0 items-center gap-1.5">
      <Button
        variant="secondary"
        size="xs"
        onClick={() => {
          openToolBesideChat(sessionId, "changes")
        }}
        className="max-w-[12rem] shrink gap-1.5 truncate font-normal"
      >
        <span className="truncate">
          {totalCount} change{totalCount === 1 ? "" : "s"}
        </span>
        <DiffStat summary={totals} />
      </Button>

      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          render={
            <Button
              variant="default"
              size="xs"
              className="shrink-0 gap-1 font-normal"
            />
          }
        >
          <GitMerge data-icon="inline-start" aria-hidden />
          {primaryLabel}
        </PopoverTrigger>
        <PopoverContent side="top" align="end" className="w-72">
          <PopoverTitle className="sr-only">Commit changes</PopoverTitle>
          <Input
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="Commit message"
            aria-label="Commit message"
            autoFocus
          />
          <div className="flex flex-wrap items-center justify-end gap-1.5">
            {hasRemote ? (
              <>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy !== null || !trimmed}
                  onClick={() => void handleCommit(false)}
                >
                  {busy === "commit" ? (
                    <Spinner data-icon="inline-start" />
                  ) : null}
                  Commit
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy !== null || !trimmed}
                  onClick={() => void handleCommit(true)}
                >
                  {busy === "push" ? (
                    <Spinner data-icon="inline-start" />
                  ) : null}
                  Commit & Push
                </Button>
                <Button
                  variant="default"
                  size="sm"
                  disabled={busy !== null || !trimmed}
                  onClick={() => {
                    setOpen(false)
                    setPrDialogOpen(true)
                  }}
                >
                  <GitPullRequest className="h-3 w-3" aria-hidden />
                  Create PR
                </Button>
              </>
            ) : (
              <Button
                variant="default"
                size="sm"
                disabled={busy !== null || !trimmed}
                onClick={() => void handleCommit(false)}
              >
                {busy === "commit" ? (
                  <Spinner data-icon="inline-start" />
                ) : null}
                Commit
              </Button>
            )}
          </div>
        </PopoverContent>
      </Popover>

      <CreatePrDialog
        open={prDialogOpen}
        initialTitle={trimmed}
        initialBody=""
        isLoading={busy === "pr"}
        onCancel={() => {
          if (busy !== "pr") setPrDialogOpen(false)
        }}
        onConfirm={(title, body) => {
          void handleCommitPr(title, body)
        }}
      />
    </div>
  )
}
