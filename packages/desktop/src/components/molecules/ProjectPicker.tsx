import { useEffect, useMemo, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { Check, ChevronDown, Folder, FolderOpen } from "lucide-react"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { createSession, toInvokeError, updateSession } from "../../lib/tauri"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../../lib/browserPreview"
import { invalidateWorkspaceQueries } from "../../lib/invalidateWorkspaceQueries"
import { DEFAULT_SESSION_TITLE } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { basename, parentPathPrefix } from "../../lib/utils"
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

type ProjectPickerProps = {
  sessionId: string | null
  cwd?: string
  disabled?: boolean
  onError?: (message: string) => void
}

const RECENT_CAP = 10

export const ProjectPicker = ({
  sessionId,
  cwd,
  disabled = false,
  onError,
}: ProjectPickerProps) => {
  const [openMenu, setOpenMenu] = useState(false)
  const [query, setQuery] = useState("")
  const [busy, setBusy] = useState(false)
  const queryClient = useQueryClient()
  const recentCwds = useAppStore((s) => s.recentCwds)
  const pushRecentCwd = useAppStore((s) => s.pushRecentCwd)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const setRoute = useAppStore((s) => s.setRoute)
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const selectedIsolation = useAppStore((s) => s.selectedIsolation)

  const label = cwd ? basename(cwd) : "Project"

  const recents = useMemo(() => {
    // Closed: skip session-cache scan — trigger only needs `label`.
    if (!openMenu) return []
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
  }, [openMenu, recentCwds, cwd, queryClient])

  const filtered = useMemo(() => {
    if (!openMenu) return []
    const q = query.trim().toLowerCase()
    if (!q) return recents
    return recents.filter(
      (p) =>
        p.toLowerCase().includes(q) || basename(p).toLowerCase().includes(q),
    )
  }, [openMenu, recents, query])

  useEffect(() => {
    if (!openMenu) setQuery("")
  }, [openMenu])

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
        })
        await queryClient.invalidateQueries({ queryKey: ["sessions"] })
        setActiveSessionId(meta.id, { panel: "closed" })
        setRoute("chat")
      }
      setOpenMenu(false)
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
    <DropdownMenu open={openMenu} onOpenChange={setOpenMenu}>
      <DropdownMenuTrigger
        disabled={disabled || busy}
        render={
          <Button
            type="button"
            variant="ghost"
            disabled={disabled || busy}
            aria-label={`Project: ${label}`}
            className="h-6 max-w-[10rem] justify-start gap-1 px-1.5 text-sm font-normal text-muted-foreground opacity-80 hover:bg-transparent hover:text-foreground hover:opacity-100 aria-expanded:opacity-100"
          />
        }
      >
        <Folder className="size-3 shrink-0" aria-hidden />
        <span className="min-w-0 truncate">{label}</span>
        <ChevronDown className="size-2.5 shrink-0" aria-hidden />
      </DropdownMenuTrigger>
      {openMenu ? (
        <DropdownMenuContent
          align="start"
          side="top"
          sideOffset={6}
          className="w-80 p-0"
        >
          <div className="border-b border-border px-2.5 py-2">
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.stopPropagation()}
              placeholder="Run agent anywhere…"
              aria-label="Search projects"
              className="h-6 w-full bg-transparent text-xs outline-none placeholder:text-muted-foreground"
            />
          </div>
          {filtered.length > 0 ? (
            <DropdownMenuGroup className="max-h-48 overflow-y-auto py-1">
              <DropdownMenuLabel>Recents</DropdownMenuLabel>
              {filtered.map((path) => {
                const active = path === cwd
                const parent = parentPathPrefix(path)
                const name = basename(path)
                return (
                  <DropdownMenuItem
                    key={path}
                    disabled={busy}
                    onClick={() => void applyCwd(path)}
                    className="mx-1"
                  >
                    <Folder className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
                    <span className="min-w-0 truncate" aria-label={path}>
                      {parent ? (
                        <span className="text-muted-foreground">{parent}</span>
                      ) : null}
                      <span className="text-foreground">{name}</span>
                    </span>
                    {active ? (
                      <Check className="ml-auto size-3 shrink-0 text-primary" aria-hidden />
                    ) : null}
                  </DropdownMenuItem>
                )
              })}
            </DropdownMenuGroup>
          ) : (
            <div className="px-2.5 py-3 text-center text-xs text-muted-foreground">
              No recent projects
            </div>
          )}
          <DropdownMenuSeparator className="m-0" />
          <DropdownMenuGroup className="py-1">
            <DropdownMenuItem
              disabled={busy}
              onClick={() => void handleOpenFolder()}
              className="mx-1"
            >
              <FolderOpen />
              Open Folder
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      ) : null}
    </DropdownMenu>
  )
}
