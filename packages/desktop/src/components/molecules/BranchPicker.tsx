import { useEffect, useMemo, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Check, GitBranch, GitPullRequest } from "lucide-react"
import { Command as CommandPrimitive } from "cmdk"
import {
  gitBranch,
  gitCheckout,
  gitHasRemote,
  gitListBranches,
  gitPrStatus,
  toInvokeError,
} from "../../lib/tauri"
import { openExternalUrl } from "../../lib/openExternalUrl"
import { fuzzyScore } from "../../lib/fuzzySearch"
import { HighlightedLabel } from "../atoms"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "../../lib/utils"

type BranchPickerProps = {
  cwd?: string
  disabled?: boolean
  onError?: (message: string) => void
}

const triggerClassName =
  "h-6 max-w-[12rem] gap-1 px-1.5 text-sm font-normal text-muted-foreground opacity-80 hover:bg-fill-4 hover:opacity-100 data-popup-open:bg-fill-4 data-popup-open:opacity-100"

/** Defer `gh pr view` until the session chrome is interactive — running it on
 * every ContextBar mount (new session / switch) stacked with git_status and
 * blocked the UI for 1–3s. */
const schedulePrPrefetch = (fn: () => void): (() => void) => {
  if (typeof requestIdleCallback === "function") {
    const id = requestIdleCallback(() => fn(), { timeout: 2_000 })
    return () => cancelIdleCallback(id)
  }
  const t = window.setTimeout(fn, 600)
  return () => window.clearTimeout(t)
}

export const BranchPicker = ({
  cwd,
  disabled = false,
  onError,
}: BranchPickerProps) => {
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [query, setQuery] = useState("")
  const [prPrefetchReady, setPrPrefetchReady] = useState(false)
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

  useEffect(() => {
    setPrPrefetchReady(false)
    if (!cwd || !hasRemote) return
    return schedulePrPrefetch(() => setPrPrefetchReady(true))
  }, [cwd, hasRemote])

  const { data: prStatus } = useQuery({
    queryKey: ["git-pr-status", cwd],
    queryFn: () => gitPrStatus(cwd!),
    // Idle-gated: Open-tab already keeps this disabled; ContextBar used to
    // fire `gh auth status` + `gh pr view` on every new-session mount.
    enabled: !!cwd && hasRemote && prPrefetchReady,
    staleTime: 60_000,
    refetchOnWindowFocus: true,
  })
  const branchPr = prStatus?.pr ?? null

  const label = current ?? "No branch"
  const canOpen = !!cwd && !disabled

  const filtered = useMemo(() => {
    const q = query.trim()
    if (!q) return branches
    return branches
      .map((branch) => ({ branch, score: fuzzyScore(q, branch) }))
      .filter(
        (r): r is { branch: string; score: number } => r.score !== null,
      )
      .sort((a, b) => a.score - b.score)
      .map((r) => r.branch)
  }, [branches, query])

  useEffect(() => {
    if (open) setQuery("")
  }, [open])

  const handleSelect = async (branch: string) => {
    if (!cwd || branch === current) {
      setOpen(false)
      return
    }
    setBusy(true)
    try {
      await gitCheckout(cwd, branch)
      await queryClient.invalidateQueries({ queryKey: ["git-branch", cwd] })
      await queryClient.invalidateQueries({ queryKey: ["git-branches", cwd] })
      await queryClient.invalidateQueries({ queryKey: ["git-pr-status", cwd] })
      setOpen(false)
    } catch (err) {
      onError?.(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="relative flex items-center gap-1">
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          disabled={!canOpen || busy}
          render={
            <Button
              type="button"
              variant="ghost"
              size="xs"
              aria-label={`Branch: ${label}`}
              title={label}
              className={cn(triggerClassName, open && "bg-fill-4 opacity-100")}
              disabled={!canOpen || busy}
            />
          }
        >
          <GitBranch
            className="size-3.5 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <span className="min-w-0 truncate">{label}</span>
        </PopoverTrigger>
        <PopoverContent
          side="top"
          align="start"
          sideOffset={4}
          className="w-72 gap-0 overflow-hidden p-0"
        >
          <PopoverTitle className="sr-only">Select branch</PopoverTitle>
          <Command
            shouldFilter={false}
            className="rounded-none bg-transparent p-0"
          >
            <div className="flex shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5 py-1.5">
              <CommandPrimitive.Input
                value={query}
                onValueChange={setQuery}
                placeholder="Search branches…"
                aria-label="Search branches"
                className={cn(
                  "h-auto min-w-0 flex-1 border-0 bg-transparent px-0 py-0 text-sm text-ink outline-hidden",
                  "rounded-none placeholder:text-ink-faint",
                )}
              />
            </div>
            <CommandList className="py-1" style={{ maxHeight: 200 }}>
              <CommandEmpty className="px-2.5 py-2 text-sm text-ink-muted">
                {isFetching ? "Loading branches…" : "No branches found"}
              </CommandEmpty>
              {filtered.length > 0 ? (
                <CommandGroup>
                  {filtered.map((branch) => {
                    const active = branch === current
                    return (
                      <CommandItem
                        key={branch}
                        value={branch}
                        disabled={busy}
                        onSelect={() => void handleSelect(branch)}
                        className="px-2.5"
                      >
                        <span className="min-w-0 flex-1 truncate">
                          {query.trim() ? (
                            <HighlightedLabel label={branch} query={query} />
                          ) : (
                            branch
                          )}
                        </span>
                        {active ? (
                          <span className="ml-auto flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                            Current
                            <Check
                              className="size-3 shrink-0 text-primary"
                              aria-hidden
                            />
                          </span>
                        ) : null}
                      </CommandItem>
                    )
                  })}
                </CommandGroup>
              ) : null}
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>

      {branchPr ? (
        <Button
          variant="ghost"
          size="xs"
          onClick={() => void openExternalUrl(branchPr.url)}
          title={`${branchPr.title} — ${branchPr.checksSummary}`}
          aria-label={`Open pull request #${branchPr.number}`}
          className="max-w-[7.5rem] gap-1 px-1.5 text-muted-foreground hover:bg-fill-4 hover:text-foreground"
        >
          <GitPullRequest className="size-3 shrink-0" aria-hidden />
          <span className="shrink-0 font-medium text-foreground">
            #{branchPr.number}
          </span>
          <span
            className={cn(
              "min-w-0 truncate",
              branchPr.checksSummary.includes("failing")
                ? "text-destructive"
                : branchPr.checksSummary.includes("pending")
                  ? "text-muted-foreground"
                  : "text-success",
            )}
          >
            {branchPr.checksSummary}
          </span>
        </Button>
      ) : null}
    </div>
  )
}
