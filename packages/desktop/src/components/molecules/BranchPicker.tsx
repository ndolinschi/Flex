import { useMemo, useRef, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Check, GitBranch, GitPullRequest } from "lucide-react"
import {
  gitBranch,
  gitCheckout,
  gitHasRemote,
  gitListBranches,
  gitPrStatus,
  toInvokeError,
} from "../../lib/tauri"
import { openExternalUrl } from "../../lib/openExternalUrl"
import { PickerTrigger } from "../atoms"
import { PopoverItem, PopoverSearch, PopoverTray } from "./PopoverTray"
import { cn } from "../../lib/utils"

type BranchPickerProps = {
  cwd?: string
  disabled?: boolean
  onError?: (message: string) => void
}

export const BranchPicker = ({
  cwd,
  disabled = false,
  onError,
}: BranchPickerProps) => {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const [busy, setBusy] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const queryClient = useQueryClient()

  const { data: current } = useQuery({
    queryKey: ["git-branch", cwd],
    queryFn: () => gitBranch(cwd ?? ""),
    enabled: !!cwd,
    staleTime: 15_000,
    retry: false,
  })

  const { data: branches = [], isFetching } = useQuery({
    queryKey: ["git-branches", cwd],
    queryFn: () => gitListBranches(cwd ?? ""),
    enabled: !!cwd && open,
    staleTime: 15_000,
    retry: false,
  })

  const { data: hasRemote = false } = useQuery({
    queryKey: ["git-has-remote", cwd],
    queryFn: () => gitHasRemote(cwd!),
    enabled: !!cwd,
    staleTime: 10_000,
  })

  const { data: prStatus } = useQuery({
    queryKey: ["git-pr-status", cwd],
    queryFn: () => gitPrStatus(cwd!),
    enabled: !!cwd && hasRemote,
    staleTime: 60_000,
    refetchOnWindowFocus: true,
  })
  const branchPr = prStatus?.pr ?? null

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return branches
    return branches.filter((b) => b.toLowerCase().includes(q))
  }, [branches, query])

  const label = current ?? "No branch"
  const canOpen = !!cwd && !disabled

  const handleClose = () => {
    setOpen(false)
    setQuery("")
  }

  const handleSelect = async (branch: string) => {
    if (!cwd || branch === current) {
      handleClose()
      return
    }
    setBusy(true)
    try {
      await gitCheckout(cwd, branch)
      await queryClient.invalidateQueries({ queryKey: ["git-branch", cwd] })
      await queryClient.invalidateQueries({ queryKey: ["git-branches", cwd] })
      await queryClient.invalidateQueries({ queryKey: ["git-pr-status", cwd] })
      handleClose()
    } catch (err) {
      onError?.(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div ref={rootRef} className="relative flex items-center gap-1">
      <PickerTrigger
        leadingIcon={<GitBranch className="h-3 w-3 shrink-0" aria-hidden />}
        label={label}
        open={open}
        onClick={() => setOpen((v) => !v)}
        disabled={!canOpen || busy}
        ariaLabel={`Branch: ${label}`}
        className="max-w-[12rem]"
      />

      {branchPr ? (
        <button
          type="button"
          onClick={() => void openExternalUrl(branchPr.url)}
          title={`${branchPr.title} — ${branchPr.checksSummary}`}
          aria-label={`Open pull request #${branchPr.number}`}
          className={cn(
            "flex h-6 max-w-[7.5rem] items-center gap-1 rounded-md px-1.5",
            "text-xs text-ink-secondary",
            "transition-colors duration-[var(--duration-fast)]",
            "hover:bg-fill-3 hover:text-ink",
          )}
        >
          <GitPullRequest className="h-3 w-3 shrink-0 text-icon-3" aria-hidden />
          <span className="shrink-0 font-medium text-ink">#{branchPr.number}</span>
          <span
            className={cn(
              "min-w-0 truncate",
              branchPr.checksSummary.includes("failing")
                ? "text-danger"
                : branchPr.checksSummary.includes("pending")
                  ? "text-ink-muted"
                  : "text-success",
            )}
          >
            {branchPr.checksSummary}
          </span>
        </button>
      ) : null}

      <PopoverTray
        open={open}
        onClose={handleClose}
        anchorRef={rootRef}
        placement="above"
        role="listbox"
        aria-label="Branches"
        className="left-0 w-72"
      >
        <PopoverSearch
          value={query}
          onChange={setQuery}
          placeholder="Search branches…"
        />
        <ul className="max-h-56 overflow-y-auto py-0.5">
          {isFetching && filtered.length === 0 ? (
            <li className="px-2.5 py-3 text-center text-xs text-ink-faint">
              Loading branches…
            </li>
          ) : filtered.length === 0 ? (
            <li className="px-2.5 py-3 text-center text-xs text-ink-faint">
              No branches found
            </li>
          ) : (
            filtered.map((branch) => {
              const active = branch === current
              return (
                <li key={branch}>
                  <PopoverItem
                    active={active}
                    disabled={busy}
                    onClick={() => void handleSelect(branch)}
                  >
                    <span className="min-w-0 flex-1 truncate">{branch}</span>
                    {active ? (
                      <span className="flex shrink-0 items-center gap-1 text-xs text-ink-faint">
                        Current
                        <Check className="h-3 w-3 text-accent" aria-hidden />
                      </span>
                    ) : null}
                  </PopoverItem>
                </li>
              )
            })
          )}
        </ul>
      </PopoverTray>
    </div>
  )
}
