import { useRef, useState } from "react"
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
import { Button, IconButton, RunningDot } from "../atoms"
import { ContextMenu, type ContextMenuItem } from "./ContextMenu"
import { PopoverItem, PopoverSearch, PopoverSection, PopoverTray } from "./PopoverTray"
import { useGroupedModels } from "../../hooks/useGroupedModels"

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

/** Compact provider-grouped model pill for the Plan tab's toolbar — same
 * grouping/search logic as `ModelPicker`/`ModelSelect` (see
 * `useGroupedModels`) but styled as a small pill trigger rather than a
 * form field or composer-pill, matching the reference header's "Grok 4.5
 * Medium" chip. */
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
  const rootRef = useRef<HTMLDivElement>(null)

  const selected = models.find((m) => m.id === value)
  const label = selected?.displayName ?? selected?.id ?? "Select model"
  const { groups } = useGroupedModels(models, query, builtinProviders)

  const handleClose = () => {
    setOpen(false)
    setQuery("")
  }

  return (
    <div ref={rootRef} className="relative shrink-0">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        disabled={isLoading}
        aria-haspopup="listbox"
        aria-expanded={open}
        className={cn(
          "inline-flex h-6 max-w-[12rem] items-center gap-1 rounded-full border border-stroke-3 px-2",
          "text-xs text-ink-secondary transition-colors duration-[var(--duration-fast)]",
          "hover:border-stroke-2 hover:text-ink disabled:opacity-50",
          open && "border-stroke-2 text-ink",
        )}
      >
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <ChevronDown className="h-2.5 w-2.5 shrink-0 text-icon-3" aria-hidden />
      </button>

      <PopoverTray
        open={open}
        onClose={handleClose}
        anchorRef={rootRef}
        placement="below"
        role="listbox"
        aria-label="Build model"
        className="right-0 left-auto w-64"
      >
        <PopoverSearch value={query} onChange={setQuery} placeholder="Search models" />
        <div className="max-h-56 overflow-y-auto py-0.5">
          {groups.length === 0 ? (
            <p className="px-2.5 py-3 text-center text-xs text-ink-faint">
              No models found
            </p>
          ) : (
            groups.map((group) => (
              <PopoverSection key={group.providerId} label={group.label}>
                <ul>
                  {group.items.map((m) => {
                    const active = m.id === value
                    return (
                      <li key={m.id}>
                        <PopoverItem
                          active={active}
                          onClick={() => {
                            onChange(m.id)
                            handleClose()
                          }}
                        >
                          <span className="min-w-0 flex-1 truncate">
                            {m.displayName ?? m.id}
                          </span>
                          <span className="flex w-3 shrink-0 items-center justify-center">
                            {active ? (
                              <Check
                                className="h-3 w-3 text-accent"
                                aria-hidden
                              />
                            ) : null}
                          </span>
                        </PopoverItem>
                      </li>
                    )
                  })}
                </ul>
              </PopoverSection>
            ))
          )}
        </div>
      </PopoverTray>
    </div>
  )
}

/** Header toolbar for the right panel's Plan tab: breadcrumbs, build model
 * pill, Build/Keep-planning actions, and a "…" overflow menu (Copy as
 * Markdown / Find in Plan / Save to Workspace). Folds the old
 * Approve/Keep-planning buttons into one bar (reference design). */
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
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null)
  const menuButtonRef = useRef<HTMLDivElement>(null)
  const findInputRef = useRef<HTMLInputElement>(null)

  const openMenu = () => {
    const rect = menuButtonRef.current?.getBoundingClientRect()
    if (!rect) return
    setMenuPos({ x: rect.right, y: rect.bottom + 4 })
  }

  const menuItems: ContextMenuItem[] = [
    {
      type: "item",
      label: "Add comment",
      icon: MessageSquareText,
      disabled: !onAddComment,
      onSelect: () => onAddComment?.(),
    },
    {
      type: "item",
      label: "Rewrite plan",
      icon: Pencil,
      disabled: !!actionsDisabled || !onRewrite,
      onSelect: () => onRewrite?.(),
    },
    {
      type: "item",
      label: "Restart / try again",
      icon: RotateCcw,
      disabled: !!actionsDisabled || !onRestart,
      onSelect: () => onRestart?.(),
    },
    { type: "separator" },
    {
      type: "item",
      label: "Copy as Markdown",
      icon: ClipboardCopy,
      onSelect: onCopyMarkdown,
    },
    {
      type: "item",
      label: "Find in Plan",
      icon: Search,
      disabled: !find,
      onSelect: () => {
        find?.onOpenChange(true)
        requestAnimationFrame(() => findInputRef.current?.focus())
      },
    },
    {
      type: "item",
      label: "Save to Workspace",
      icon: Save,
      disabled: !!saveDisabled,
      onSelect: onSaveToWorkspace,
    },
  ]

  return (
    <div className={cn("flex shrink-0 flex-col", className)}>
      <div className="flex h-[var(--header-height)] items-center gap-1.5 px-2 text-sm">
        <span className="min-w-0 truncate text-ink-muted">{repo}</span>
        <span className="text-ink-faint">›</span>
        {showPlansListCrumb && onBackToPlans ? (
          <button
            type="button"
            onClick={onBackToPlans}
            className="text-ink-muted transition-colors duration-[var(--duration-fast)] hover:text-ink"
          >
            Plans
          </button>
        ) : (
          <span className="text-ink-muted">Plans</span>
        )}
        <span className="text-ink-faint">›</span>
        <span className="min-w-0 truncate text-ink-secondary">{title}</span>

        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          <PlanModelPill
            models={models}
            builtinProviders={builtinProviders}
            value={modelId}
            onChange={onModelChange}
            isLoading={modelsLoading}
          />

          {showKeepPlanning && onKeepPlanning ? (
            <Button variant="ghost" size="sm" onClick={onKeepPlanning}>
              Keep planning
            </Button>
          ) : null}

          {status === "built" ? (
            <span
              className="flex h-7 items-center gap-1 rounded-md px-2 text-sm text-yellow"
              data-testid="plan-build-status"
            >
              <Check className="h-3 w-3" aria-hidden /> Built
            </span>
          ) : status === "building" ? (
            <span
              className="flex h-7 items-center gap-1.5 rounded-md px-2 text-sm text-ink-secondary"
              data-testid="plan-build-status"
            >
              <RunningDot className="h-4 w-4" /> Building…
            </span>
          ) : (
            <Button
              variant="primary"
              size="sm"
              disabled={status === "draft"}
              onClick={onBuild}
              aria-label="Build plan"
            >
              <Hammer className="h-3.5 w-3.5" aria-hidden />
              Build
            </Button>
          )}

          <div ref={menuButtonRef} className="shrink-0">
            <IconButton
              label="More plan actions"
              onClick={openMenu}
              className={cn(menuPos && "bg-fill-3 text-ink")}
            >
              <MoreHorizontal className="h-3.5 w-3.5" aria-hidden />
            </IconButton>
          </div>
        </span>
      </div>

      {find?.open ? (
        <div className="flex h-8 items-center gap-1.5 border-t border-stroke-3 px-2">
          <Search className="h-3.5 w-3.5 shrink-0 text-icon-3" aria-hidden />
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
            className="h-6 min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-ink-faint focus:outline-none focus-visible:[box-shadow:inset_0_0_0_1px_var(--color-stroke-2)]"
          />
          <span className="shrink-0 text-xs tabular-nums text-ink-faint">
            {find.matchCount > 0 ? `${find.activeIndex + 1}/${find.matchCount}` : "0/0"}
          </span>
          <IconButton label="Previous match" onClick={find.onPrev} disabled={find.matchCount === 0} className="h-6 w-6">
            <ChevronDown className="h-3 w-3 rotate-180" aria-hidden />
          </IconButton>
          <IconButton label="Next match" onClick={find.onNext} disabled={find.matchCount === 0} className="h-6 w-6">
            <ChevronDown className="h-3 w-3" aria-hidden />
          </IconButton>
          <IconButton label="Close find" onClick={() => find.onOpenChange(false)} className="h-6 w-6">
            <X className="h-3 w-3" aria-hidden />
          </IconButton>
        </div>
      ) : null}

      <ContextMenu
        position={menuPos}
        items={
          saveDisabled && saveDisabledReason
            ? menuItems.map((item) =>
                item.type === "item" && item.label === "Save to Workspace"
                  ? { ...item, label: `Save to Workspace (${saveDisabledReason})` }
                  : item,
              )
            : menuItems
        }
        onClose={() => setMenuPos(null)}
      />
    </div>
  )
}
