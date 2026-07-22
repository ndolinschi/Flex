import { useEffect, useMemo, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { Check, Folder, FolderOpen } from "lucide-react"
import { Command as CommandPrimitive } from "cmdk"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { createSession, toInvokeError, updateSession } from "../../lib/tauri"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../../lib/browserPreview"
import { invalidateWorkspaceQueries } from "../../lib/invalidateWorkspaceQueries"
import { DEFAULT_SESSION_TITLE } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { upsertSessionInCache } from "../../hooks/useSessions"
import { basename, parentPathPrefix, cn } from "../../lib/utils"
import { fuzzyScore } from "../../lib/fuzzySearch"
import { HighlightedLabel } from "../atoms"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command"
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover"

type ProjectPickerProps = {
  sessionId: string | null
  cwd?: string
  disabled?: boolean
  onError?: (message: string) => void
}

const RECENT_CAP = 10

const triggerClassName =
  "h-6 max-w-[10rem] gap-1 px-1.5 text-sm font-normal text-muted-foreground opacity-80 hover:bg-fill-4 hover:opacity-100 data-popup-open:bg-fill-4 data-popup-open:opacity-100"

/** Score a project path against the query — basename preferred, full path as fallback. */
const pathFuzzyScore = (query: string, path: string): number | null => {
  const name = basename(path)
  const byName = fuzzyScore(query, name)
  const byPath = fuzzyScore(query, path)
  if (byName == null) return byPath
  if (byPath == null) return byName
  return Math.min(byName, byPath)
}

export const ProjectPicker = ({
  sessionId,
  cwd,
  disabled = false,
  onError,
}: ProjectPickerProps) => {
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [query, setQuery] = useState("")
  const queryClient = useQueryClient()
  const recentCwds = useAppStore((s) => s.recentCwds)
  const pushRecentCwd = useAppStore((s) => s.pushRecentCwd)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const selectedIsolation = useAppStore((s) => s.selectedIsolation)
  const selectedReuseWorkspaceId = useAppStore(
    (s) => s.selectedReuseWorkspaceId,
  )

  const label = cwd ? basename(cwd) : "Project"

  const recents = useMemo(() => {
    // Closed: skip session-cache scan — trigger only needs `label`.
    if (!open) return []
    const sessions =
      queryClient.getQueryData<
        { cwd: string; base_cwd?: string; parent_id?: string }[]
      >(["sessions"]) ?? []
    // Skip subagent children — their worktree paths aren't user projects.
    // Prefer base_cwd so isolated sessions don't pollute recents with UUID worktrees.
    const fromSessions = sessions
      .filter((s) => !s.parent_id)
      .map((s) => s.base_cwd || s.cwd)
    const projectCwd = cwd // caller should pass base_cwd ?? cwd
    const merged = [...recentCwds, ...fromSessions, projectCwd].filter(
      (p): p is string => !!p && p.trim().length > 0,
    )
    const seen = new Set<string>()
    const unique: string[] = []
    for (const path of merged) {
      if (seen.has(path)) continue
      seen.add(path)
      unique.push(path)
    }
    return unique.slice(0, RECENT_CAP)
  }, [open, recentCwds, cwd, queryClient])

  const filtered = useMemo(() => {
    const q = query.trim()
    if (!q) return recents
    return recents
      .map((path) => ({ path, score: pathFuzzyScore(q, path) }))
      .filter(
        (r): r is { path: string; score: number } => r.score !== null,
      )
      .sort((a, b) => a.score - b.score)
      .map((r) => r.path)
  }, [recents, query])

  useEffect(() => {
    if (open) setQuery("")
  }, [open])

  const applyCwd = async (nextCwd: string) => {
    setBusy(true)
    try {
      pushRecentCwd(nextCwd)
      if (sessionId) {
        await updateSession(sessionId, { cwd: nextCwd })
        await queryClient.invalidateQueries({ queryKey: ["sessions"] })
        await queryClient.invalidateQueries({ queryKey: ["git-branch", nextCwd] })
        await queryClient.invalidateQueries({
          queryKey: ["git-branches", nextCwd],
        })
        await queryClient.invalidateQueries({
          queryKey: ["git-is-repo", nextCwd],
        })
        await queryClient.invalidateQueries({
          queryKey: ["git-has-remote", nextCwd],
        })
        invalidateWorkspaceQueries(queryClient)
      } else {
        const meta = await createSession({
          title: DEFAULT_SESSION_TITLE,
          cwd: nextCwd,
          model: selectedModelId ?? undefined,
          ...(selectedIsolation ? { isolation: selectedIsolation } : {}),
          ...(selectedReuseWorkspaceId && selectedIsolation
            ? { reuseWorkspaceId: selectedReuseWorkspaceId }
            : {}),
        })
        upsertSessionInCache(queryClient, meta)
        setActiveSessionId(meta.id, { panel: "closed" })
        setRoute("chat")
        void queryClient.invalidateQueries({ queryKey: ["sessions"] })
      }
      setOpen(false)
    } catch (err) {
      onError?.(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  const handleOpenFolder = async () => {
    try {
      if (isBrowserPreview()) {
        onError?.(NATIVE_APP_REQUIRED)
        return
      }
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "Open Folder",
      })
      if (!selected || Array.isArray(selected)) return
      await applyCwd(selected)
    } catch (err) {
      onError?.(toInvokeError(err))
    }
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        disabled={disabled || busy}
        render={
          <Button
            type="button"
            variant="ghost"
            size="xs"
            aria-label={`Project: ${label}`}
            title={cwd ?? "Select project"}
            className={cn(triggerClassName, open && "bg-fill-4 opacity-100")}
            disabled={disabled || busy}
          />
        }
      >
        <Folder className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <span className="min-w-0 truncate">{label}</span>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="start"
        sideOffset={4}
        className="w-80 gap-0 overflow-hidden p-0"
      >
        <PopoverTitle className="sr-only">Select project</PopoverTitle>
        <Command
          shouldFilter={false}
          className="rounded-none bg-transparent p-0"
        >
          <div className="flex shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5 py-1.5">
            {/* cmdk Input (not CommandInput) — CommandInput wraps an inset
             * InputGroup that reads as a nested chip in this chrome strip. */}
            <CommandPrimitive.Input
              value={query}
              onValueChange={setQuery}
              placeholder="Search projects…"
              aria-label="Search projects"
              className={cn(
                "h-auto min-w-0 flex-1 border-0 bg-transparent px-0 py-0 text-sm text-ink outline-hidden",
                "rounded-none placeholder:text-ink-faint",
              )}
            />
          </div>
          <CommandList className="py-1" style={{ maxHeight: 200 }}>
            <CommandEmpty className="px-2.5 py-2 text-sm text-ink-muted">
              No recent projects
            </CommandEmpty>
            {filtered.length > 0 ? (
              <CommandGroup heading={query.trim() ? undefined : "Recents"}>
                {filtered.map((path) => {
                  const parent = parentPathPrefix(path)
                  const name = basename(path)
                  const active = path === cwd
                  return (
                    <CommandItem
                      key={path}
                      value={path}
                      disabled={busy}
                      onSelect={() => void applyCwd(path)}
                      className="px-2.5"
                    >
                      <Folder
                        className="size-3.5 shrink-0 text-muted-foreground"
                        aria-hidden
                      />
                      <span className="min-w-0 flex-1 truncate" aria-label={path}>
                        {parent ? (
                          <span className="text-muted-foreground">{parent}</span>
                        ) : null}
                        <span className="text-foreground">
                          {query.trim() ? (
                            <HighlightedLabel label={name} query={query} />
                          ) : (
                            name
                          )}
                        </span>
                      </span>
                      {active ? (
                        <Check
                          className="size-3.5 shrink-0 text-primary"
                          aria-hidden
                        />
                      ) : null}
                    </CommandItem>
                  )
                })}
              </CommandGroup>
            ) : null}
          </CommandList>
          <CommandSeparator />
          <div className="p-1">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="w-full justify-start"
              disabled={busy}
              onClick={() => void handleOpenFolder()}
            >
              <FolderOpen />
              Open Folder
            </Button>
          </div>
        </Command>
      </PopoverContent>
    </Popover>
  )
}
