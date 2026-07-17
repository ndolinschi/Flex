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
} from "@/components/icons"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button, IconButton, RunningDot } from "../atoms"
import { ContextMenu, type ContextMenuItem } from "./ContextMenu"
import { useGroupedModels } from "../../hooks/useGroupedModels"
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"
import {
  Combobox,
  ComboboxCollection,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxTrigger,
} from "@/components/ui/combobox"

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

  const selected = models.find((m) => m.id === value) ?? null
  const label = selected?.displayName ?? selected?.id ?? "Select model"
  const { groups, providerLabel } = useGroupedModels(
    models,
    "",
    builtinProviders,
  )

  return (
    <Combobox
      items={groups}
      value={selected}
      onValueChange={(next) => {
        if (!next) return
        onChange(next.id)
        setOpen(false)
      }}
      open={open}
      onOpenChange={setOpen}
      disabled={isLoading}
      itemToStringLabel={(m) => m.displayName ?? m.id}
      isItemEqualToValue={(a, b) => a.id === b.id}
      filter={(item, query) => {
        const q = query.trim().toLowerCase()
        if (!q) return true
        const m = item as ModelInfoDto
        return (
          m.id.toLowerCase().includes(q) ||
          (m.displayName?.toLowerCase().includes(q) ?? false) ||
          m.providerId.toLowerCase().includes(q) ||
          providerLabel(m.providerId).toLowerCase().includes(q)
        )
      }}
    >
      <ComboboxTrigger
        hideIcon
        disabled={isLoading}
        className={cn(
          "inline-flex h-6 max-w-[12rem] shrink-0 items-center gap-1 rounded-full border border-stroke-3 px-2",
          "text-xs text-ink-secondary shadow-none transition-colors duration-[var(--duration-fast)]",
          "hover:border-stroke-2 hover:bg-transparent hover:text-ink disabled:opacity-50",
          open && "border-stroke-2 text-ink",
        )}
      >
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <ChevronDown className="h-2.5 w-2.5 shrink-0 text-icon-3" aria-hidden />
      </ComboboxTrigger>
      <ComboboxContent align="end" sideOffset={4} className="w-64 min-w-64">
        <ComboboxInput
          placeholder="Search models"
          showTrigger={false}
          className="w-full"
        />
        <ComboboxEmpty>No models found</ComboboxEmpty>
        <ComboboxList className="max-h-56">
          {(group) => (
            <ComboboxGroup key={group.providerId} items={group.items}>
              <ComboboxLabel>{group.label}</ComboboxLabel>
              <ComboboxCollection>
                {(m: ModelInfoDto) => (
                  <ComboboxItem key={m.id} value={m}>
                    <span className="min-w-0 flex-1 truncate">
                      {m.displayName ?? m.id}
                    </span>
                    {m.id === value ? (
                      <Check className="h-3 w-3 text-accent" aria-hidden />
                    ) : null}
                  </ComboboxItem>
                )}
              </ComboboxCollection>
            </ComboboxGroup>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
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
    <div className={cn("flex shrink-0 flex-col border-b border-stroke-3", className)}>
      <div className="flex h-[var(--header-height)] items-center gap-1.5 px-2.5 text-sm">
        <Breadcrumb className="min-w-0 flex-1 overflow-hidden">
          <BreadcrumbList className="flex-nowrap gap-1.5 overflow-hidden">
            <BreadcrumbItem className="min-w-0">
              <span className="truncate text-ink-muted">{repo}</span>
            </BreadcrumbItem>
            <BreadcrumbSeparator />
            <BreadcrumbItem className="shrink-0">
              {showPlansListCrumb && onBackToPlans ? (
                <BreadcrumbLink
                  asChild
                  className="text-ink-muted hover:text-ink"
                >
                  <button type="button" onClick={onBackToPlans}>
                    Plans
                  </button>
                </BreadcrumbLink>
              ) : (
                <BreadcrumbPage className="text-ink-muted">Plans</BreadcrumbPage>
              )}
            </BreadcrumbItem>
            <BreadcrumbSeparator />
            <BreadcrumbItem className="min-w-0">
              <BreadcrumbPage className="truncate">{title}</BreadcrumbPage>
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
              <Check className="h-3 w-3" aria-hidden /> Built
            </span>
          ) : status === "building" ? (
            <span
              className="flex h-6 items-center gap-1.5 rounded-md px-2 text-sm text-ink-secondary"
              data-testid="plan-build-status"
            >
              <RunningDot className="h-4 w-4" /> Building…
            </span>
          ) : (
            <Button
              variant="primary"
              size="sm"
              className="h-6"
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
              className={cn("h-6 w-6", menuPos && "bg-fill-3 text-ink")}
            >
              <MoreHorizontal className="h-3.5 w-3.5" aria-hidden />
            </IconButton>
          </div>
        </span>
      </div>

      {find?.open ? (
        <div className="flex h-8 items-center gap-1.5 border-t border-stroke-3 px-2.5">
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
