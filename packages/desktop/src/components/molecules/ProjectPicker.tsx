import { useMemo, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { Folder, FolderOpen } from "lucide-react"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { createSession, toInvokeError, updateSession } from "../../lib/tauri"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../../lib/browserPreview"
import { invalidateWorkspaceQueries } from "../../lib/invalidateWorkspaceQueries"
import { DEFAULT_SESSION_TITLE } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { upsertSessionInCache } from "../../hooks/useSessions"
import { basename, parentPathPrefix } from "../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxSeparator,
  ComboboxTrigger,
} from "@/components/ui/combobox"
import { contextBarTriggerClass } from "../organisms/context-bar/chrome"

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
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
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
    if (!open) return []
    const sessions =
      queryClient.getQueryData<
        { cwd: string; base_cwd?: string; parent_id?: string }[]
      >(["sessions"]) ?? []
    const fromSessions = sessions
      .filter((s) => !s.parent_id)
      .map((s) => s.base_cwd || s.cwd)
    const projectCwd = cwd
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
    <Combobox
      items={open ? recents : []}
      value={cwd ?? null}
      onValueChange={(next) => {
        if (typeof next === "string" && next) void applyCwd(next)
      }}
      itemToStringLabel={(path) => basename(path)}
      open={open}
      onOpenChange={setOpen}
      disabled={disabled || busy}
    >
      <ComboboxTrigger
        aria-label={`Project: ${label}`}
        disabled={disabled || busy}
        className={contextBarTriggerClass()}
      >
        <Folder className="size-3.5 text-ink-muted" aria-hidden />
        <span className="min-w-0 truncate">{label}</span>
      </ComboboxTrigger>
      <ComboboxContent className="w-80" side="top" align="start">
        <ComboboxInput
          placeholder="Search projects…"
          aria-label="Search projects"
          showTrigger={false}
          disabled={disabled || busy}
        />
        <ComboboxEmpty>No recent projects</ComboboxEmpty>
        {recents.length > 0 ? (
          <ComboboxGroup>
            <ComboboxLabel>Recents</ComboboxLabel>
            <ComboboxList>
              {(path) => {
                const parent = parentPathPrefix(path)
                const name = basename(path)
                return (
                  <ComboboxItem key={path} value={path} disabled={busy}>
                    <Folder
                      className="size-3.5 shrink-0 text-ink-muted"
                      aria-hidden
                    />
                    <span className="min-w-0 truncate" aria-label={path}>
                      {parent ? (
                        <span className="text-ink-muted">{parent}</span>
                      ) : null}
                      <span className="text-foreground">{name}</span>
                    </span>
                  </ComboboxItem>
                )
              }}
            </ComboboxList>
          </ComboboxGroup>
        ) : null}
        <ComboboxSeparator />
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
      </ComboboxContent>
    </Combobox>
  )
}
