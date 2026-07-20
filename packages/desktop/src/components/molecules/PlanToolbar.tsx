import { useEffect, useRef, useState } from "react"
import {
  Check,
  ChevronDown,
  ClipboardCopy,
  Hammer,
  MessageSquareText,
  MoreHorizontal,
  Pencil,
  RotateCcw,
  Save,
  Search,
  X,
} from "lucide-react"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"
import { RunningDot, TextInput } from "../atoms"
import { useGroupedModels, MODEL_MENU_VISIBLE_CAP } from "../../hooks/useGroupedModels"

export type PlanBuildStatus = "draft" | "ready" | "building" | "built"

type PlanToolbarProps = {
  /** repo basename (breadcrumb root). */
  repo: string
  /** Truncated title — first heading of the plan doc, or the session title. */
  title: string
  /** When true, the "Plans" crumb is a button that returns to the multi-plan list. */
  showPlansListCrumb?: boolean
  onBackToPlans?: () => void
  models: ModelInfoDto[]
  builtinProviders?: BuiltinProvider[]
  modelId: string | null
  onModelChange: (id: string) => void
  modelsLoading?: boolean
  status: PlanBuildStatus
  onBuild: () => void
  onKeepPlanning?: () => void
  /** Only rendered when Keep-planning applies (awaiting approval). */
  showKeepPlanning?: boolean
  onCopyMarkdown: () => void
  /** `null` disables Find-in-Plan entirely (no plan doc yet). */
  find: {
    query: string
    onQueryChange: (q: string) => void
    matchCount: number
    activeIndex: number
    onNext: () => void
    onPrev: () => void
    open: boolean
    onOpenChange: (open: boolean) => void
  } | null
  onSaveToWorkspace: () => void
  saveDisabled?: boolean
  saveDisabledReason?: string
  onRewrite?: () => void
  onRestart?: () => void
  /** Opens the select-to-comment flow (not an agent review prompt). */
  onAddComment?: () => void
  actionsDisabled?: boolean
  className?: string
}

/** Compact provider-grouped model pill for the Plan tab's toolbar. */
const PlanModelPill = ({
  models,
  builtinProviders = [],
  value,
  onChange,
  isLoading,
}: {
  models: ModelInfoDto[]
  builtinProviders?: BuiltinProvider[]
  value: string | null
  onChange: (id: string) => void
  isLoading?: boolean
}) => {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")

  const selected = models.find((m) => m.id === value)
  const label = selected?.displayName ?? selected?.id ?? "Select model"
  const { groups, truncated, totalMatched } = useGroupedModels(
    models,
    query,
    builtinProviders,
    open,
  )

  useEffect(() => {
    if (!open) setQuery("")
  }, [open])

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger
        disabled={isLoading}
        render={
          <Button
            type="button"
            variant="ghost"
            size="xs"
            disabled={isLoading}
            className={cn(
              "max-w-[12rem] rounded-full border border-border px-2 text-muted-foreground",
              "transition-colors duration-[var(--duration-fast)]",
              "hover:border-border hover:bg-transparent hover:text-foreground",
              "aria-expanded:border-border aria-expanded:text-foreground",
            )}
          />
        }
      >
        <span className="min-w-0 truncate">{label}</span>
        <ChevronDown className="size-2.5 shrink-0 text-muted-foreground" aria-hidden />
      </DropdownMenuTrigger>
      {open ? (
        <DropdownMenuContent align="end" sideOffset={4} className="w-64 p-0">
          <div className="border-b border-border px-2.5 py-2">
            <TextInput
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder="Search models"
              aria-label="Search models"
              className="h-6 border-0 bg-transparent px-0 text-xs shadow-none focus-visible:ring-0 rounded-none"
            />
          </div>
          <div className="max-h-56 overflow-y-auto py-1">
            {groups.length === 0 ? (
              <p className="px-2.5 py-3 text-center text-xs text-muted-foreground">
                No models found
              </p>
            ) : (
              groups.map((group) => (
                <DropdownMenuGroup key={group.providerId}>
                  <DropdownMenuLabel>{group.label}</DropdownMenuLabel>
                  {group.items.map((m) => {
                    const active = m.id === value
                    return (
                      <DropdownMenuItem
                        key={m.id}
                        className="mx-1"
                        onClick={() => {
                          onChange(m.id)
                          setOpen(false)
                        }}
                      >
                        <span className="min-w-0 truncate">
                          {m.displayName ?? m.id}
                        </span>
                        {active ? (
                          <Check className="ml-auto size-3 text-primary" aria-hidden />
                        ) : null}
                      </DropdownMenuItem>
                    )
                  })}
                </DropdownMenuGroup>
              ))
            )}
            {truncated ? (
              <p className="px-2.5 py-2 text-xs text-muted-foreground">
                Showing {MODEL_MENU_VISIBLE_CAP} of {totalMatched}. Type to
                narrow.
              </p>
            ) : null}
          </div>
        </DropdownMenuContent>
      ) : null}
    </DropdownMenu>
  )
}

/** Header toolbar for the right panel's Plan tab: breadcrumbs, build model
 * pill, Build/Keep-planning actions, and a "…" overflow menu. */
export const PlanToolbar = ({
  repo,
  title,
  showPlansListCrumb,
  onBackToPlans,
  models,
  builtinProviders,
  modelId,
  onModelChange,
  modelsLoading,
  status,
  onBuild,
  onKeepPlanning,
  showKeepPlanning,
  onCopyMarkdown,
  find,
  onSaveToWorkspace,
  saveDisabled,
  saveDisabledReason,
  onRewrite,
  onRestart,
  onAddComment,
  actionsDisabled,
  className,
}: PlanToolbarProps) => {
  const [menuOpen, setMenuOpen] = useState(false)
  const findInputRef = useRef<HTMLInputElement>(null)

  const saveLabel =
    saveDisabled && saveDisabledReason
      ? `Save to Workspace (${saveDisabledReason})`
      : "Save to Workspace"

  return (
    <div className={cn("flex shrink-0 flex-col", className)}>
      <div className="flex h-[var(--header-height)] items-center gap-1.5 px-2.5 text-sm">
        <Breadcrumb className="min-w-0 flex-1 overflow-hidden">
          <BreadcrumbList className="flex-nowrap overflow-hidden gap-1.5">
            <BreadcrumbItem className="min-w-0 shrink overflow-hidden">
              <BreadcrumbPage className="min-w-0 truncate text-muted-foreground font-normal">
                {repo}
              </BreadcrumbPage>
            </BreadcrumbItem>
            <BreadcrumbSeparator className="shrink-0 text-muted-foreground/60">
              ›
            </BreadcrumbSeparator>
            <BreadcrumbItem className="shrink-0">
              {showPlansListCrumb && onBackToPlans ? (
                <BreadcrumbLink
                  render={
                    <button
                      type="button"
                      onClick={onBackToPlans}
                      className="text-muted-foreground hover:text-foreground"
                    />
                  }
                >
                  Plans
                </BreadcrumbLink>
              ) : (
                <span className="text-muted-foreground">Plans</span>
              )}
            </BreadcrumbItem>
            <BreadcrumbSeparator className="shrink-0 text-muted-foreground/60">
              ›
            </BreadcrumbSeparator>
            <BreadcrumbItem className="min-w-0 shrink overflow-hidden">
              <BreadcrumbPage className="min-w-0 truncate text-foreground/80 font-normal">
                {title}
              </BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>

        <span className="flex shrink-0 items-center gap-1.5">
          <PlanModelPill
            models={models}
            builtinProviders={builtinProviders}
            value={modelId}
            onChange={onModelChange}
            isLoading={modelsLoading}
          />

          {showKeepPlanning && onKeepPlanning ? (
            <Button
              variant="ghost"
              size="sm"
              className="h-6"
              onClick={onKeepPlanning}
            >
              Keep planning
            </Button>
          ) : null}

          {status === "built" ? (
            <span
              className="flex h-6 items-center gap-1 rounded-md px-2 text-sm text-yellow"
              data-testid="plan-build-status"
            >
              <Check className="size-3" aria-hidden /> Built
            </span>
          ) : status === "building" ? (
            <span
              className="flex h-6 items-center gap-1.5 rounded-md px-2 text-sm text-muted-foreground"
              data-testid="plan-build-status"
            >
              <RunningDot className="h-4 w-4" /> Building…
            </span>
          ) : (
            <Button
              variant="default"
              size="sm"
              className="h-6"
              disabled={status === "draft"}
              onClick={onBuild}
              aria-label="Build plan"
            >
              <Hammer className="size-3.5" aria-hidden />
              Build
            </Button>
          )}

          <DropdownMenu open={menuOpen} onOpenChange={setMenuOpen}>
            <DropdownMenuTrigger
              render={
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label="More plan actions"
                  className="size-6"
                />
              }
            >
              <MoreHorizontal className="size-3.5" aria-hidden />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" sideOffset={4} className="w-56">
              <DropdownMenuGroup>
                <DropdownMenuItem
                  disabled={!onAddComment}
                  onClick={() => onAddComment?.()}
                >
                  <MessageSquareText />
                  Add comment
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={!!actionsDisabled || !onRewrite}
                  onClick={() => onRewrite?.()}
                >
                  <Pencil />
                  Rewrite plan
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={!!actionsDisabled || !onRestart}
                  onClick={() => onRestart?.()}
                >
                  <RotateCcw />
                  Restart / try again
                </DropdownMenuItem>
              </DropdownMenuGroup>
              <DropdownMenuSeparator />
              <DropdownMenuGroup>
                <DropdownMenuItem onClick={onCopyMarkdown}>
                  <ClipboardCopy />
                  Copy as Markdown
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={!find}
                  onClick={() => {
                    find?.onOpenChange(true)
                    requestAnimationFrame(() => findInputRef.current?.focus())
                  }}
                >
                  <Search />
                  Find in Plan
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={!!saveDisabled}
                  onClick={onSaveToWorkspace}
                >
                  <Save />
                  {saveLabel}
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </span>
      </div>

      {find?.open ? (
        <div className="flex h-8 items-center gap-1.5 border-y border-border px-2.5">
          <Search className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
          <input
            ref={findInputRef}
            type="text"
            value={find.query}
            onChange={(e) => find.onQueryChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault()
                find.onOpenChange(false)
                return
              }
              if (e.key === "Enter") {
                e.preventDefault()
                if (e.shiftKey) find.onPrev()
                else find.onNext()
              }
            }}
            placeholder="Find in plan"
            aria-label="Find in plan"
            className="h-6 min-w-0 flex-1 bg-transparent text-sm text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {find.matchCount > 0 ? `${find.activeIndex + 1}/${find.matchCount}` : "0/0"}
          </span>
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Previous match" title="Previous match"
      onClick={find.onPrev}
      disabled={find.matchCount === 0}
      className={cn(
        "text-muted-foreground hover:bg-accent hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <ChevronDown className="size-3 rotate-180" aria-hidden />
    </Button>
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Next match" title="Next match"
      onClick={find.onNext}
      disabled={find.matchCount === 0}
      className={cn(
        "text-muted-foreground hover:bg-accent hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <ChevronDown className="size-3" aria-hidden />
    </Button>
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Close find" title="Close find"
      onClick={() => find.onOpenChange(false)}
      className={cn(
        "text-muted-foreground hover:bg-accent hover:text-foreground",
        "h-6 w-6",
      )}
    >
      <X className="size-3" aria-hidden />
    </Button>
        </div>
      ) : null}
    </div>
  )
}
