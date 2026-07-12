import { useMemo, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { Check, Folder, FolderOpen } from "lucide-react"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { createSession, toInvokeError, updateSession } from "../../lib/tauri"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../../lib/browserPreview"
import { DEFAULT_SESSION_TITLE } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { basename } from "../../lib/utils"
import { PickerTrigger } from "../atoms"
import {
  PopoverItem,
  PopoverSearch,
  PopoverSection,
  PopoverTray,
} from "./PopoverTray"

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
  const rootRef = useRef<HTMLDivElement>(null)
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
      queryClient.getQueryData<{ cwd: string; parent_id?: string }[]>([
        "sessions",
      ]) ?? []
    // Skip subagent children — their worktree paths aren't user projects.
    const fromSessions = sessions.filter((s) => !s.parent_id).map((s) => s.cwd)
    const merged = [...recentCwds, ...fromSessions, cwd].filter(
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

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return recents
    return recents.filter(
      (p) =>
        p.toLowerCase().includes(q) || basename(p).toLowerCase().includes(q),
    )
  }, [recents, query])

  const handleClose = () => {
    setOpenMenu(false)
    setQuery("")
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
      } else {
        const meta = await createSession({
          title: DEFAULT_SESSION_TITLE,
          cwd: nextCwd,
          model: selectedModelId ?? undefined,
          ...(selectedIsolation ? { isolation: selectedIsolation } : {}),
        })
        await queryClient.invalidateQueries({ queryKey: ["sessions"] })
        setActiveSessionId(meta.id)
        setRoute("chat")
      }
      handleClose()
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
    <div ref={rootRef} className="relative">
      <PickerTrigger
        leadingIcon={<Folder className="h-3 w-3 shrink-0" aria-hidden />}
        label={label}
        open={openMenu}
        onClick={() => setOpenMenu((v) => !v)}
        disabled={disabled || busy}
        ariaLabel={`Project: ${label}`}
        className="max-w-[10rem]"
      />

      <PopoverTray
        open={openMenu}
        onClose={handleClose}
        anchorRef={rootRef}
        placement="above"
        role="listbox"
        aria-label="Projects"
        className="left-0 w-80"
      >
        <PopoverSearch
          value={query}
          onChange={setQuery}
          placeholder="Run agent anywhere…"
        />
        {filtered.length > 0 ? (
          <PopoverSection label="Recents">
            <ul className="max-h-48 overflow-y-auto">
              {filtered.map((path) => {
                const active = path === cwd
                return (
                  <li key={path}>
                    <PopoverItem
                      active={active}
                      disabled={busy}
                      onClick={() => void applyCwd(path)}
                    >
                      <Folder
                        className="h-3.5 w-3.5 shrink-0 text-icon-3"
                        aria-hidden
                      />
                      <span className="min-w-0 flex-1 truncate">
                        <span className="text-ink-faint">
                          {path.replace(/\/[^/]+\/?$/, "/")}
                        </span>
                        <span className="text-ink">{basename(path)}</span>
                      </span>
                      {active ? (
                        <Check
                          className="h-3 w-3 shrink-0 text-accent"
                          aria-hidden
                        />
                      ) : null}
                    </PopoverItem>
                  </li>
                )
              })}
            </ul>
          </PopoverSection>
        ) : (
          <p className="px-2.5 py-3 text-center text-xs text-ink-faint">
            No recent projects
          </p>
        )}
        <div className="border-t border-stroke-3 py-0.5">
          <PopoverItem
            role="menuitem"
            disabled={busy}
            onClick={() => void handleOpenFolder()}
          >
            <FolderOpen className="h-3.5 w-3.5" aria-hidden />
            Open Folder
          </PopoverItem>
        </div>
      </PopoverTray>
    </div>
  )
}
