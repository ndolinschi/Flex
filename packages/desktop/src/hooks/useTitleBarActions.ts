import { useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { openUrl } from "@tauri-apps/plugin-opener"
import { closeWindow } from "../lib/windowChrome"
import {
  findDraftSession,
  newAgentCreateInput,
  resolveCreateCwd,
} from "../lib/sessions"
import {
  createSession,
  resumeSession,
  toInvokeError,
} from "../lib/tauri"
import type { SessionMeta } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import { SESSIONS_KEY } from "./useSessions"

const DOCS_URL = "https://github.com/ndolinschi/Flex#readme"
const ISSUES_URL = "https://github.com/ndolinschi/Flex/issues"

export type TitleBarActionHandlers = {
  newAgent: () => void
  openFolder: () => void
  settings: () => void
  quit: () => void
  search: () => void
  commandPalette: () => void
  toggleSidebar: () => void
  togglePanel: () => void
  toggleTheme: () => void
  docs: () => void
  submitBug: () => void
  issues: () => void
}

type UseTitleBarActionsOpts = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  onOpenBugReport: () => void
}

/**
 * Shared File/Edit/View/Help actions for in-window menus and the native macOS
 * menu bar. Handlers are referentially stable (refs) so the title bar does not
 * thrash child menus on every chat/tab switch.
 */
export const useTitleBarActions = ({
  onOpenCommandPalette,
  onOpenSearch,
  onOpenBugReport,
}: UseTitleBarActionsOpts): {
  isBootstrapped: boolean
  handlers: TitleBarActionHandlers
} => {
  const queryClient = useQueryClient()
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)

  const optsRef = useRef({
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport,
    isBootstrapped,
  })
  optsRef.current = {
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport,
    isBootstrapped,
  }

  const handlersRef = useRef<TitleBarActionHandlers | null>(null)
  if (!handlersRef.current) {
    handlersRef.current = {
      newAgent: () => {
        if (!optsRef.current.isBootstrapped) return
        void (async () => {
          const state = useAppStore.getState()
          const sessions =
            queryClient.getQueryData<SessionMeta[]>(SESSIONS_KEY) ?? []
          const cwd = resolveCreateCwd(
            sessions,
            state.activeSessionId,
            state.recentCwds,
          )
          const draft = findDraftSession(sessions, cwd)
          if (draft) {
            try {
              await resumeSession(draft.id)
            } catch {
              // Still select locally if resume fails (session already warm).
            }
            state.setActiveSessionId(draft.id, { panel: "closed" })
            state.setRoute("chat")
            return
          }
          try {
            const meta = await createSession(
              newAgentCreateInput(
                cwd,
                state.selectedModelId,
                state.selectedIsolation,
              ),
            )
            void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
            state.setActiveSessionId(meta.id, { panel: "closed" })
            state.setRoute("chat")
          } catch (err) {
            state.pushToast(
              `Could not create agent: ${toInvokeError(err)}`,
              "error",
            )
          }
        })()
      },
      openFolder: () => {
        void (async () => {
          if (!optsRef.current.isBootstrapped) return
          const state = useAppStore.getState()
          try {
            const path = await openDialog({ directory: true, multiple: false })
            if (!path || Array.isArray(path)) return
            state.pushRecentCwd(path)
            const meta = await createSession(newAgentCreateInput(path))
            void queryClient.invalidateQueries({ queryKey: SESSIONS_KEY })
            state.setActiveSessionId(meta.id, { panel: "closed" })
            state.setRoute("chat")
          } catch (err) {
            state.pushToast(
              `Could not open folder: ${toInvokeError(err)}`,
              "error",
            )
          }
        })()
      },
      settings: () => {
        if (!optsRef.current.isBootstrapped) return
        useAppStore.getState().setRoute("settings")
      },
      quit: () => {
        void closeWindow()
      },
      search: () => {
        optsRef.current.onOpenSearch?.()
      },
      commandPalette: () => {
        optsRef.current.onOpenCommandPalette?.()
      },
      toggleSidebar: () => {
        if (!optsRef.current.isBootstrapped) return
        useAppStore.getState().toggleSidebarCollapsed()
      },
      togglePanel: () => {
        if (!optsRef.current.isBootstrapped) return
        useAppStore.getState().toggleRightPanel()
      },
      toggleTheme: () => {
        useAppStore.getState().toggleTheme()
      },
      docs: () => {
        void openUrl(DOCS_URL).catch(() => undefined)
      },
      submitBug: () => {
        optsRef.current.onOpenBugReport()
      },
      issues: () => {
        void openUrl(ISSUES_URL).catch(() => undefined)
      },
    }
  }

  return {
    isBootstrapped,
    handlers: handlersRef.current,
  }
}
