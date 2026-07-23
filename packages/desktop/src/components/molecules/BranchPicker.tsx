import { useEffect, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { GitBranch, GitPullRequest } from "lucide-react"
import {
  gitBranch,
  gitCheckout,
  gitHasRemote,
  gitListBranches,
  gitPrStatus,
  toInvokeError,
} from "../../lib/tauri"
import { openExternalUrl } from "../../lib/openExternalUrl"
import { Button } from "@/components/ui/button"
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
} from "@/components/ui/combobox"
import { cn } from "../../lib/utils"
import { contextBarTriggerClass } from "../organisms/context-bar/chrome"

type BranchPickerProps = {
  cwd?: string
  disabled?: boolean
  onError?: (message: string) => void
}

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
    enabled: !!cwd && hasRemote && prPrefetchReady,
    staleTime: 60_000,
    refetchOnWindowFocus: true,
  })
  const branchPr = prStatus?.pr ?? null

  const label = current ?? "No branch"
  const canOpen = !!cwd && !disabled

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
      <Combobox
        items={open ? branches : []}
        value={current ?? null}
        onValueChange={(next) => {
          if (typeof next === "string" && next) void handleSelect(next)
        }}
        open={open}
        onOpenChange={setOpen}
        disabled={!canOpen || busy}
      >
        <ComboboxTrigger
          aria-label={`Branch: ${label}`}
          disabled={!canOpen || busy}
          className={contextBarTriggerClass("max-w-[12rem]")}
        >
          <GitBranch className="size-3.5 text-ink-muted" aria-hidden />
          <span className="min-w-0 truncate">{label}</span>
        </ComboboxTrigger>
        <ComboboxContent className="w-72" side="top" align="start">
          <ComboboxInput
            placeholder="Search branches…"
            aria-label="Search branches"
            showTrigger={false}
            disabled={!canOpen || busy}
          />
          <ComboboxEmpty>
            {isFetching ? "Loading branches…" : "No branches found"}
          </ComboboxEmpty>
          <ComboboxList>
            {(branch) => {
              const active = branch === current
              return (
                <ComboboxItem key={branch} value={branch} disabled={busy}>
                  <span className="min-w-0 truncate">{branch}</span>
                  {active ? (
                    <span className="ml-auto flex shrink-0 items-center gap-1 text-xs text-ink-muted">
                      Current
                    </span>
                  ) : null}
                </ComboboxItem>
              )
            }}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>

      {branchPr ? (
        <Button
          variant="ghost"
          size="xs"
          onClick={() => void openExternalUrl(branchPr.url)}
          title={`${branchPr.title} — ${branchPr.checksSummary}`}
          aria-label={`Open pull request #${branchPr.number}`}
          className="max-w-[7.5rem] gap-1 px-1.5 text-ink-muted hover:bg-fill-4 hover:text-ink"
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
                  ? "text-ink-muted"
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
