import { useRef, useState } from "react"
import {
  Check,
  ClipboardCopy,
  Hammer,
  MessageSquareText,
  MoreHorizontal,
  Pencil,
  RotateCcw,
  Save,
  Search,
} from "lucide-react"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"
import { cn } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
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
import { RunningDot } from "../atoms"
import { PlanModelPill } from "./PlanModelPill"
import { PlanFindBar, type PlanFindState } from "./PlanFindBar"

export type PlanBuildStatus = "draft" | "ready" | "building" | "built"

type PlanToolbarProps = {
  repo: string
  title: string
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
  showKeepPlanning?: boolean
  onCopyMarkdown: () => void
  find: PlanFindState | null
  onSaveToWorkspace: () => void
  saveDisabled?: boolean
  saveDisabledReason?: string
  onRewrite?: () => void
  onRestart?: () => void
  onAddComment?: () => void
  actionsDisabled?: boolean
  className?: string
}

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
              <BreadcrumbPage className="min-w-0 truncate text-ink-muted font-normal">
                {repo}
              </BreadcrumbPage>
            </BreadcrumbItem>
            <BreadcrumbSeparator className="shrink-0 text-ink-muted/60">
              ›
            </BreadcrumbSeparator>
            <BreadcrumbItem className="shrink-0">
              {showPlansListCrumb && onBackToPlans ? (
                <BreadcrumbLink
                  render={
                    <Button
                      type="button"
                      variant="link"
                      onClick={onBackToPlans}
                      className="h-auto p-0 text-ink-muted hover:text-ink"
                    />
                  }
                >
                  Plans
                </BreadcrumbLink>
              ) : (
                <span className="text-ink-muted">Plans</span>
              )}
            </BreadcrumbItem>
            <BreadcrumbSeparator className="shrink-0 text-ink-muted/60">
              ›
            </BreadcrumbSeparator>
            <BreadcrumbItem className="min-w-0 shrink overflow-hidden">
              <BreadcrumbPage className="min-w-0 truncate text-ink/80 font-normal">
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
              className="flex h-6 items-center gap-1.5 rounded-md px-2 text-sm text-ink-muted"
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

      {find?.open ? <PlanFindBar find={find} inputRef={findInputRef} /> : null}
    </div>
  )
}
