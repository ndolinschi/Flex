import { useEffect, useMemo, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Check, ChevronDown, GitBranch, GitPullRequest } from "lucide-react"
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
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

  useEffect(() => {
    if (!open) setQuery("")
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
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger
          disabled={!canOpen || busy}
          render={
            <Button
              type="button"
              variant="ghost"
              disabled={!canOpen || busy}
              aria-label={`Branch: ${label}`}
              className={cn(
                "h-6 max-w-[12rem] gap-1 px-1.5 font-normal",
                "text-sm text-muted-foreground opacity-80",
                "hover:bg-transparent hover:text-foreground hover:opacity-100",
                "aria-expanded:opacity-100",
              )}
            />
          }
        >
          <GitBranch className="size-3 shrink-0" aria-hidden />
          <span className="min-w-0 truncate">{label}</span>
          <ChevronDown className="size-2.5 shrink-0" aria-hidden />
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          side="top"
          sideOffset={6}
          className="w-72 p-0"
        >
          <div className="border-b border-border px-2.5 py-2">
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder="Search branches…"
              aria-label="Search branches"
              className="h-6 w-full bg-transparent text-xs outline-none placeholder:text-muted-foreground"
            />
          </div>
          <DropdownMenuGroup className="max-h-56 overflow-y-auto py-1">
            {isFetching && filtered.length === 0 ? (
              <div className="px-2.5 py-3 text-center text-xs text-muted-foreground">
                Loading branches…
              </div>
            ) : filtered.length === 0 ? (
              <div className="px-2.5 py-3 text-center text-xs text-muted-foreground">
                No branches found
              </div>
            ) : (
              filtered.map((branch) => {
                const active = branch === current
                return (
                  <DropdownMenuItem
                    key={branch}
                    disabled={busy}
                    onClick={() => void handleSelect(branch)}
                    className="mx-1"
                  >
                    <span className="min-w-0 truncate">{branch}</span>
                    {active ? (
                      <span className="ml-auto flex shrink-0 items-center gap-1 text-xs text-muted-foreground">
                        Current
                        <Check className="size-3 text-primary" aria-hidden />
                      </span>
                    ) : null}
                  </DropdownMenuItem>
                )
              })
            )}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      {branchPr ? (
        <Button
          variant="ghost"
          size="xs"
          onClick={() => void openExternalUrl(branchPr.url)}
          title={`${branchPr.title} — ${branchPr.checksSummary}`}
          aria-label={`Open pull request #${branchPr.number}`}
          className="max-w-[7.5rem] gap-1 px-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
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
