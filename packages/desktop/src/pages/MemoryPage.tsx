import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Brain, ChevronDown, Clock, FileText, MoreHorizontal, Trash2 } from "lucide-react"
import { IconButton, Spinner, Tooltip } from "../components/atoms"
import {
  Collapsible,
  ConfirmDialog,
  ContextMenu,
  type ContextMenuItem,
  EmptyState,
  ErrorBanner,
  MarkdownBody,
} from "../components/molecules"
import { SettingsShell } from "../components/templates"
import { useSessions } from "../hooks/useSessions"
import {
  memoryGet,
  memoryList,
  memoryRemove,
  memorySetExpiry,
  projectMemoryGet,
  projectMemoryList,
  projectMemoryRemove,
  projectMemorySetExpiry,
  toInvokeError,
} from "../lib/tauri"
import type { MemoryEntryDto, MemoryTtlPreset } from "../lib/types"
import { memoryExpiryFromPreset } from "../lib/types"
import { basename, cn, formatCountdown, formatRelativeTime } from "../lib/utils"

const MEMORY_KEY = ["memory"] as const
const projectMemoryKey = (cwd: string) => ["project-memory", cwd] as const

const EMPTY_MEMORIES: MemoryEntryDto[] = []

type MemoryPageProps = {
  embedded?: boolean
}

type MemoryScope = {
  getMemory: (id: string) => Promise<MemoryEntryDto>
  removeMemory: (id: string) => Promise<void>
  setExpiry: (id: string, expiresAtMs: number | undefined) => Promise<void>
  invalidateKey: readonly unknown[]
}

const TTL_PRESETS: { preset: MemoryTtlPreset; label: string }[] = [
  { preset: "forever", label: "Keep forever" },
  { preset: "1d", label: "1 day" },
  { preset: "1w", label: "1 week" },
  { preset: "30d", label: "30 days" },
]

/** Expiry countdown pill — faint by default, warmer red once inside the
 * last 24h. Absent entirely when the entry never expires. */
const ExpiryPill = ({ expiresAtMs }: { expiresAtMs: number }) => {
  const urgent = expiresAtMs - Date.now() < 24 * 60 * 60 * 1000
  return (
    <span
      className={cn(
        "shrink-0 whitespace-nowrap text-[11px]",
        urgent ? "text-red" : "text-ink-faint",
      )}
    >
      {formatCountdown(expiresAtMs)}
    </span>
  )
}

const MemoryRow = ({
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
      <div className="group/row relative flex min-h-[30px] items-center gap-2 px-2.5 py-1">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex shrink-0 items-center gap-1.5 rounded-sm p-0.5 text-ink-muted transition-colors hover:text-ink"
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
        </button>

        <Tooltip label={memory.title}>
          <p className="min-w-0 flex-1 truncate text-[13px] text-ink">{memory.title}</p>
        </Tooltip>

        <span className="flex shrink-0 items-center gap-2 pl-2">
          {memory.expiresAtMs ? <ExpiryPill expiresAtMs={memory.expiresAtMs} /> : null}
          {memory.updatedAtMs ? (
            <span
              className={cn(
                "whitespace-nowrap text-[11px] text-ink-faint",
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
            "pointer-events-none bg-panel transition-[max-width,opacity] duration-[100ms] ease-[var(--easing-default)]",
            "group-hover/row:pointer-events-auto group-hover/row:max-w-[76px] group-hover/row:opacity-100",
            "group-focus-within/row:pointer-events-auto group-focus-within/row:max-w-[76px] group-focus-within/row:opacity-100",
          )}
        >
          <Tooltip label="Expiry">
            <IconButton
              label="Set memory expiry"
              className="!h-6 !w-6"
              isLoading={expiryMutation.isPending}
              onClick={(e) => {
                e.stopPropagation()
                const rect = e.currentTarget.getBoundingClientRect()
                setMenuPosition({ x: rect.left, y: rect.bottom })
              }}
            >
              <MoreHorizontal className="h-3 w-3" aria-hidden />
            </IconButton>
          </Tooltip>
          <Tooltip label="Delete">
            <IconButton
              label="Delete memory"
              className="!h-6 !w-6 hover:!text-red"
              onClick={(e) => {
                e.stopPropagation()
                setConfirmDelete(true)
              }}
            >
              <Trash2 className="h-3 w-3" aria-hidden />
            </IconButton>
          </Tooltip>
        </span>
      </div>

      <Collapsible open={expanded}>
        <div className="border-t border-stroke-3 px-2.5 py-2">
          {detailQuery.isLoading ? (
            <div className="flex items-center gap-2 py-2 text-xs text-ink-muted">
              <Spinner size="sm" /> Loading…
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

/** One collapsible "Global" or per-project section, styled like a Settings
 * section header (title + entry count), holding a list of `MemoryRow`s
 * scoped to whichever get/remove/expiry functions the caller passes in. */
const MemorySection = ({
  title,
  hint,
  memories,
  scope,
  defaultOpen = true,
}: {
  title: string
  hint?: string
  memories: MemoryEntryDto[]
  scope: MemoryScope
  defaultOpen?: boolean
}) => {
  const [open, setOpen] = useState(defaultOpen)

  return (
    <section className="mb-6">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="mb-2 flex w-full items-center gap-2 text-left"
      >
        <ChevronDown
          className={cn(
            "h-3 w-3 shrink-0 text-ink-muted transition-transform duration-[var(--duration-fast)]",
            !open && "-rotate-90",
          )}
          aria-hidden
        />
        <h2 className="text-[13px] font-medium text-ink">{title}</h2>
        <span className="text-[11px] text-ink-faint">{memories.length}</span>
        {hint ? (
          <span className="truncate text-[11px] text-ink-faint">{hint}</span>
        ) : null}
      </button>
      <Collapsible open={open}>
        <div className="flex flex-col gap-1.5 pl-5">
          {memories.map((memory) => (
            <MemoryRow key={memory.id} memory={memory} scope={scope} />
          ))}
        </div>
      </Collapsible>
    </section>
  )
}

/** Discover distinct project cwds from live sessions — top-level only
    (subagent children carry a `parent_id` and their worktree paths aren't
    user projects), deduped in first-seen order. */
const useProjectCwds = (): string[] => {
  const { sessions } = useSessions()
  return useMemo(() => {
    const seen = new Set<string>()
    const cwds: string[] = []
    for (const session of sessions) {
      if (session.parent_id) continue
      if (!session.cwd || seen.has(session.cwd)) continue
      seen.add(session.cwd)
      cwds.push(session.cwd)
    }
    return cwds
  }, [sessions])
}

/** One project's memory section — fetches lazily and renders nothing until
    the query resolves with at least one entry, so the page doesn't show an
    empty section for every repo the user has ever opened a session in. */
const ProjectMemorySection = ({ cwd }: { cwd: string }) => {
  const queryKey = projectMemoryKey(cwd)
  const query = useQuery({
    queryKey,
    queryFn: () => projectMemoryList(cwd),
  })

  // Errors surface via console/query devtools rather than a per-project
  // banner — a project with an unreadable `.agent/memory` dir just renders
  // no section, consistent with "only show sections that have entries."
  if (query.isLoading || query.isError) return null
  const memories = query.data ?? EMPTY_MEMORIES
  if (memories.length === 0) return null

  const scope: MemoryScope = {
    getMemory: (id) => projectMemoryGet(cwd, id),
    removeMemory: (id) => projectMemoryRemove(cwd, id),
    setExpiry: (id, expiresAtMs) => projectMemorySetExpiry(cwd, id, expiresAtMs),
    invalidateKey: queryKey,
  }

  const dimPrefix = cwd.replace(/\/[^/]+\/?$/, "/")

  return (
    <MemorySection
      title={basename(cwd)}
      hint={dimPrefix}
      memories={memories}
      scope={scope}
      defaultOpen={false}
    />
  )
}

/** Memory browser — durable notes the `learning` plugin's `MemoryWrite` tool
    persisted, loaded into every future session's system prompt. Global
    memory lives in `~/.config/agentloop/memory/*.md` and loads everywhere;
    per-project memory lives in `<cwd>/.agent/memory/*.md` and only loads for
    sessions in that project. Read-only from the UI's perspective aside from
    delete and expiry: memories are written by the agent, not authored here —
    and for now the agent only ever writes to the global store, never
    per-project. */
export const MemoryPage = ({ embedded = false }: MemoryPageProps) => {
  const memoryQuery = useQuery({
    queryKey: MEMORY_KEY,
    queryFn: memoryList,
  })
  const projectCwds = useProjectCwds()

  const memories = memoryQuery.data ?? EMPTY_MEMORIES

  const globalScope: MemoryScope = {
    getMemory: memoryGet,
    removeMemory: memoryRemove,
    setExpiry: memorySetExpiry,
    invalidateKey: MEMORY_KEY,
  }

  const isEmpty = !memoryQuery.isLoading && !memoryQuery.isError && memories.length === 0

  return (
    <SettingsShell title="Memory" wide embedded={embedded}>
      <div className="mb-4">
        <p className="text-xs text-ink-muted">
          Durable notes the agent saves as it works — user preferences, project facts,
          environment quirks. They load into every future session automatically. New
          memories are always written to the global store; per-project notes shown
          below are read-only from here for now. Set an expiry on any note to make it
          short-term — it's purged automatically once it lapses.
        </p>
      </div>

      <section className="mb-6">
        <div className="mb-2 flex items-center gap-2">
          <h2 className="text-[13px] font-medium text-ink">Global</h2>
          <span className="text-[11px] text-ink-faint">{memories.length}</span>
        </div>
        {memoryQuery.isLoading ? (
          <div className="flex items-center gap-2 py-8 text-xs text-ink-muted">
            <Spinner size="sm" /> Loading memory…
          </div>
        ) : memoryQuery.isError ? (
          <ErrorBanner message={toInvokeError(memoryQuery.error)} />
        ) : isEmpty ? (
          <EmptyState
            icon={<FileText className="h-5 w-5" aria-hidden />}
            title="No memories yet"
            description="The agent saves reusable knowledge here as it works."
          />
        ) : (
          <div className="flex flex-col gap-1.5 pl-5">
            {memories.map((memory) => (
              <MemoryRow key={memory.id} memory={memory} scope={globalScope} />
            ))}
          </div>
        )}
      </section>

      {projectCwds.map((cwd) => (
        <ProjectMemorySection key={cwd} cwd={cwd} />
      ))}
    </SettingsShell>
  )
}
