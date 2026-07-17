import { useMemo, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { Check, Folder, FolderOpen } from "@/components/icons"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { createSession, toInvokeError, updateSession } from "../../lib/tauri"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../../lib/browserPreview"
import { invalidateWorkspaceQueries } from "../../lib/invalidateWorkspaceQueries"
import { DEFAULT_SESSION_TITLE } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { basename, parentPathPrefix } from "../../lib/utils"
import { PickerTrigger } from "../atoms"
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
} from "@/components/ui/combobox"
import { cn } from "@/lib/utils"

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
  }, [recentCwds, cwd, queryClient, openMenu])

  const handleOpenChange = (next: boolean) => {
    setOpenMenu(next)
  }

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
      handleOpenChange(false)
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
    <Combobox
      items={recents}
      value={cwd ?? null}
      onValueChange={(path) => {
        if (path) void applyCwd(path)
      }}
      open={openMenu}
      onOpenChange={handleOpenChange}
      disabled={disabled || busy}
      filter={(item, query) => {
        const q = query.trim().toLowerCase()
        if (!q) return true
        const path = String(item)
        return (
          path.toLowerCase().includes(q) ||
          basename(path).toLowerCase().includes(q)
        )
      }}
    >
      <ComboboxTrigger
        disabled={disabled || busy}
        className="border-0 bg-transparent p-0 shadow-none hover:bg-transparent data-pressed:bg-transparent"
        render={
          <PickerTrigger
            leadingIcon={<Folder className="h-3 w-3 shrink-0" aria-hidden />}
            label={label}
            open={openMenu}
            disabled={disabled || busy}
            ariaLabel={`Project: ${label}`}
            className="max-w-[10rem]"
          />
        }
      />
      <ComboboxContent
        side="top"
        align="start"
        sideOffset={6}
        className="w-80 min-w-80"
      >
        <ComboboxInput
          placeholder="Run agent anywhere…"
          showTrigger={false}
          disabled={busy}
          className="w-full"
        />
        <ComboboxEmpty>No recent projects</ComboboxEmpty>
        <ComboboxList className="max-h-48">
          {(path) => {
            const active = path === cwd
            const parent = parentPathPrefix(String(path))
            const name = basename(String(path))
            return (
              <ComboboxItem key={String(path)} value={path} disabled={busy}>
                <Folder
                  className="h-3.5 w-3.5 shrink-0 text-icon-3"
                  aria-hidden
                />
                <span className="min-w-0 flex-1 truncate" title={String(path)}>
                  {parent ? (
                    <span className="text-ink-faint">{parent}</span>
                  ) : null}
                  <span className="text-ink">{name}</span>
                </span>
                {active ? (
                  <Check className="h-3 w-3 shrink-0 text-accent" aria-hidden />
                ) : null}
              </ComboboxItem>
            )
          }}
        </ComboboxList>
        <div className="border-t border-stroke-3 py-0.5">
          <button
            type="button"
            disabled={busy}
            onClick={() => void handleOpenFolder()}
            className={cn(
              "flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-sm text-ink-secondary",
              "transition-colors duration-[var(--duration-fast)]",
              "hover:bg-[color:var(--color-select-hover)] focus:bg-[color:var(--color-select-hover)] focus:outline-none",
              "disabled:opacity-50",
            )}
          >
            <FolderOpen className="h-3.5 w-3.5" aria-hidden />
            Open Folder
          </button>
        </div>
      </ComboboxContent>
    </Combobox>
  )
}
