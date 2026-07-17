import { useMemo, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Check, GitBranch, GitPullRequest } from "@/components/icons"
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
import { cn } from "../../lib/utils"
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
} from "@/components/ui/combobox"

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
  const [busy, setBusy] = useState(false)
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
    staleTime: 30_000,
    refetchInterval: 60_000,
    refetchOnWindowFocus: true,
  })
  const branchPr = prStatus?.pr ?? null

  const label = current ?? "No branch"
  const canOpen = !!cwd && !disabled
  const items = useMemo(() => branches, [branches])

  const handleOpenChange = (next: boolean) => {
    setOpen(next)
  }

  const handleSelect = async (branch: string | null) => {
    if (!branch || !cwd) {
      handleOpenChange(false)
      return
    }
    if (branch === current) {
      handleOpenChange(false)
      return
    }
    setBusy(true)
    try {
      await gitCheckout(cwd, branch)
      await queryClient.invalidateQueries({ queryKey: ["git-branch", cwd] })
      await queryClient.invalidateQueries({ queryKey: ["git-branches", cwd] })
      await queryClient.invalidateQueries({ queryKey: ["git-pr-status", cwd] })
      handleOpenChange(false)
    } catch (err) {
      onError?.(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="relative flex items-center gap-1">
      <Combobox
        items={items}
        value={current ?? null}
        onValueChange={(value) => void handleSelect(value)}
        open={open}
        onOpenChange={handleOpenChange}
        disabled={!canOpen || busy}
      >
        <ComboboxTrigger
          disabled={!canOpen || busy}
          className="border-0 bg-transparent p-0 shadow-none hover:bg-transparent data-pressed:bg-transparent"
          render={
            <PickerTrigger
              leadingIcon={<GitBranch className="h-3 w-3 shrink-0" aria-hidden />}
              label={label}
              open={open}
              disabled={!canOpen || busy}
              ariaLabel={`Branch: ${label}`}
              className="max-w-[12rem]"
            />
          }
        />
        <ComboboxContent
          side="top"
          align="start"
          sideOffset={6}
          className="w-72 min-w-72"
        >
          <ComboboxInput
            placeholder="Search branches…"
            showTrigger={false}
            disabled={busy}
            className="w-full"
          />
          <ComboboxEmpty>
            {isFetching ? "Loading branches…" : "No branches found"}
          </ComboboxEmpty>
          <ComboboxList>
            {(branch) => (
              <ComboboxItem key={String(branch)} value={branch} disabled={busy}>
                <span className="min-w-0 flex-1 truncate">{String(branch)}</span>
                {branch === current ? (
                  <span className="flex shrink-0 items-center gap-1 text-xs text-ink-faint">
                    Current
                    <Check className="h-3 w-3 text-accent" aria-hidden />
                  </span>
                ) : null}
              </ComboboxItem>
            )}
          </ComboboxList>
        </ComboboxContent>
      </Combobox>

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
    </div>
  )
}
