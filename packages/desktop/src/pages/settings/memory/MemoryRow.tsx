import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Brain, ChevronDown, Clock, MoreHorizontal, Trash2 } from "lucide-react"
import { Tooltip } from "../../../components/atoms"
import {
  Collapsible,
  ConfirmDialog,
  ContextMenu,
  type ContextMenuItem,
  ErrorBanner,
  MarkdownBody,
} from "../../../components/molecules"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { toInvokeError } from "../../../lib/tauri"
import { memoryExpiryFromPreset } from "../../../lib/types"
import { cn, formatRelativeTime } from "../../../lib/utils"
import { TTL_PRESETS, type MemoryScope } from "./constants"
import { ExpiryPill } from "./ExpiryPill"
import type { MemoryEntryDto } from "../../../lib/types"

export const MemoryRow = ({
  memory,
  scope,
}: {
  memory: MemoryEntryDto
  scope: MemoryScope
}) => {
  const queryClient = useQueryClient()
  const [expanded, setExpanded] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(
    null,
  )
  /** Defer hover icon actions until first pointer/focus — sticky thereafter. */
  const [actionsReady, setActionsReady] = useState(false)

  const detailQuery = useQuery({
    queryKey: [...scope.invalidateKey, "detail", memory.id],
    queryFn: () => scope.getMemory(memory.id),
    enabled: expanded,
  })

  const removeMutation = useMutation({
    mutationFn: () => scope.removeMemory(memory.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: scope.invalidateKey })
      setConfirmDelete(false)
    },
  })

  const expiryMutation = useMutation({
    mutationFn: (expiresAtMs: number | undefined) =>
      scope.setExpiry(memory.id, expiresAtMs),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: scope.invalidateKey })
    },
  })

  const expiryMenuItems: ContextMenuItem[] = TTL_PRESETS.map(({ preset, label }) => ({
    type: "item",
    label,
    icon: preset === "forever" ? undefined : Clock,
    onSelect: () => expiryMutation.mutate(memoryExpiryFromPreset(preset)),
  }))

  return (
    <div className="rounded-md border border-stroke-3 bg-panel">
      <div
        className="group/row relative flex min-h-[30px] items-center gap-2 px-2.5 py-1"
        onPointerEnter={() => setActionsReady(true)}
        onFocusCapture={() => setActionsReady(true)}
      >
        <Button
          variant="ghost"
          onClick={() => setExpanded((v) => !v)}
          className="h-auto shrink-0 gap-1.5 rounded-sm p-0.5 font-normal text-ink-muted hover:bg-transparent hover:text-ink"
          aria-label={expanded ? "Collapse memory" : "Expand memory"}
          aria-expanded={expanded}
        >
          <ChevronDown
            className={cn(
              "h-3 w-3 shrink-0 transition-transform duration-[var(--duration-fast)]",
              expanded && "rotate-180",
            )}
            aria-hidden
          />
          <Brain className="h-3.5 w-3.5 shrink-0" aria-hidden />
        </Button>

        <Tooltip label={memory.title}>
          <p className="min-w-0 flex-1 truncate text-base text-ink">{memory.title}</p>
        </Tooltip>

        <span className="flex shrink-0 items-center gap-2 pl-2">
          {memory.expiresAtMs ? <ExpiryPill expiresAtMs={memory.expiresAtMs} /> : null}
          {memory.updatedAtMs ? (
            <span
              className={cn(
                "whitespace-nowrap text-xs text-ink-faint",
                "group-hover/row:hidden group-focus-within/row:hidden",
              )}
            >
              {formatRelativeTime(memory.updatedAtMs)}
            </span>
          ) : null}
        </span>

        {/* Absolutely positioned trailing actions (SessionListItem pattern) so
            hover controls never inflate this row's ~30px height. */}
        <span
          className={cn(
            "absolute right-2 top-1/2 flex max-w-0 -translate-y-1/2 items-center gap-0.5 overflow-hidden opacity-0",
            "pointer-events-none bg-panel transition-[max-width,opacity] duration-[var(--duration-fast)] ease-[var(--easing-default)]",
            "group-hover/row:pointer-events-auto group-hover/row:max-w-[76px] group-hover/row:opacity-100",
            "group-focus-within/row:pointer-events-auto group-focus-within/row:max-w-[76px] group-focus-within/row:opacity-100",
          )}
        >
          {actionsReady ? (
            <>
              <Tooltip label="Expiry">
                <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label="Set memory expiry" title="Set memory expiry"
      onClick={(e) => {
                    e.stopPropagation()
                    const rect = e.currentTarget.getBoundingClientRect()
                    setMenuPosition({ x: rect.left, y: rect.bottom })
                  }}
      disabled={expiryMutation.isPending}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
      )}
    >
      {expiryMutation.isPending ? <Spinner /> : (
        <MoreHorizontal className="h-3 w-3" aria-hidden />
      )}
    </Button>
              </Tooltip>
              <Tooltip label="Delete">
                <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label="Delete memory" title="Delete memory"
      onClick={(e) => {
                    e.stopPropagation()
                    setConfirmDelete(true)
                  }}
      className={cn(
        "text-muted-foreground hover:bg-muted hover:text-foreground",
        "hover:text-destructive",
      )}
    >
      <Trash2 className="h-3 w-3" aria-hidden />
    </Button>
              </Tooltip>
            </>
          ) : null}
        </span>
      </div>

      <Collapsible open={expanded}>
        <div className="border-t border-stroke-3 px-2.5 py-2">
          {detailQuery.isLoading ? (
            <div className="flex items-center gap-2 py-2 text-xs text-ink-muted">
              <Spinner className="size-3.5" /> Loading…
            </div>
          ) : detailQuery.isError ? (
            <ErrorBanner message={toInvokeError(detailQuery.error)} />
          ) : detailQuery.data?.content ? (
            <div className="max-h-64 overflow-y-auto rounded border border-stroke-3 bg-surface-muted/40 px-3 py-2">
              <MarkdownBody content={detailQuery.data.content} />
            </div>
          ) : (
            <p className="py-2 text-xs text-ink-faint">Empty note.</p>
          )}
        </div>
      </Collapsible>

      <ContextMenu
        position={menuPosition}
        items={expiryMenuItems}
        onClose={() => setMenuPosition(null)}
      />

      <ConfirmDialog
        open={confirmDelete}
        title={`Delete "${memory.title}"?`}
        description="This removes the memory permanently. The agent will no longer see it in future sessions."
        confirmLabel="Delete"
        danger
        isLoading={removeMutation.isPending}
        onConfirm={() => void removeMutation.mutateAsync()}
        onCancel={() => setConfirmDelete(false)}
      />
    </div>
  )
}
