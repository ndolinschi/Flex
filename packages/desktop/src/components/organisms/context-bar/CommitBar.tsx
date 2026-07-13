import { useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitMerge } from "lucide-react"
import {
  gitCommit,
  gitHasRemote,
  gitPush,
  gitStatusSinceBaseline,
  toInvokeError,
} from "../../../lib/tauri"
import { invalidateGitQueries } from "../../../lib/invalidateGitQueries"
import { useAppStore } from "../../../stores/appStore"
import { cn } from "../../../lib/utils"
import { PopoverTray } from "../../molecules/PopoverTray"
import { Button, DiffStat, TextInput } from "../../atoms"

/** Right-aligned "N changes" pill + Commit button, shown above the
 * composer for non-isolated sessions with a dirty working tree (design:
 * "Changes +9745 -737" pill + button). Clicking the pill jumps to the
 * Changes tab; the button opens an inline popover to compose the message.
 *
 * Label / actions depend on remotes: no remote → Commit only; with a
 * remote → Commit and Commit & Push. */
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
  const [message, setMessage] = useState("Update from agent session")
  const [busy, setBusy] = useState<"commit" | "push" | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()
  const pushToast = useAppStore((s) => s.pushToast)
  const setRightPanelOpen = useAppStore((s) => s.setRightPanelOpen)
  const setRightPanelTab = useAppStore((s) => s.setRightPanelTab)

  const { data: summary } = useQuery({
    queryKey: ["git-status", cwd ?? "", sessionId ?? null],
    queryFn: () => gitStatusSinceBaseline(sessionId),
    enabled: !!cwd && !!sessionId,
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

  const handleCommit = async (andPush: boolean) => {
    if (busy) return
    if (andPush && !hasRemote) return
    setBusy("commit")
    try {
      // TODO: gitCommit stages the whole repo (`git add -A` in the Rust
      // `git_commit` command) even though the count/list above is
      // session-scoped (gitStatusSinceBaseline). A session with 0 tracked
      // changes can still commit unrelated pre-existing dirty files repo-wide.
      const sha = await gitCommit(sessionId, message.trim())
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

  if (totalCount === 0) return null

  return (
    <div ref={rootRef} className="relative flex shrink-0 items-center gap-1.5">
      <button
        type="button"
        onClick={() => {
          setRightPanelOpen(true)
          setRightPanelTab("changes")
        }}
        className={cn(
          "flex h-6 max-w-[12rem] shrink items-center gap-1.5 truncate rounded-md",
          "bg-fill-3 px-2 text-xs text-ink-muted whitespace-nowrap",
          "transition-colors hover:bg-fill-2 hover:text-ink-secondary",
        )}
      >
        <span className="truncate">
          {totalCount} change{totalCount === 1 ? "" : "s"}
        </span>
        <DiffStat summary={totals} />
      </button>

      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className={cn(
          "flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-md",
          "bg-accent px-2 text-xs text-accent-text",
          "transition-colors hover:bg-accent-hover",
        )}
      >
        <GitMerge className="h-3 w-3 shrink-0" aria-hidden />
        {primaryLabel}
      </button>

      <PopoverTray
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={rootRef}
        placement="above"
        role="dialog"
        aria-label="Commit changes"
        className="right-0 w-72 p-2.5"
      >
        <TextInput
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="Commit message"
          aria-label="Commit message"
          autoFocus
        />
        <div className="mt-2 flex items-center justify-end gap-1.5">
          {hasRemote ? (
            <>
              <Button
                variant="secondary"
                size="sm"
                isLoading={busy === "commit"}
                disabled={busy !== null || !message.trim()}
                onClick={() => void handleCommit(false)}
              >
                Commit
              </Button>
              <Button
                variant="primary"
                size="sm"
                isLoading={busy === "push"}
                disabled={busy !== null || !message.trim()}
                onClick={() => void handleCommit(true)}
              >
                Commit & Push
              </Button>
            </>
          ) : (
            <Button
              variant="primary"
              size="sm"
              isLoading={busy === "commit"}
              disabled={busy !== null || !message.trim()}
              onClick={() => void handleCommit(false)}
            >
              Commit
            </Button>
          )}
        </div>
      </PopoverTray>
    </div>
  )
}
