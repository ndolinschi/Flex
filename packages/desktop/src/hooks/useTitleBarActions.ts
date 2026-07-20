import { useRef } from "react"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { openUrl } from "@tauri-apps/plugin-opener"
import { closeWindow } from "../lib/windowChrome"
import { newAgentCreateInput } from "../lib/sessions"
import { createSession, toInvokeError } from "../lib/tauri"
import { useAppStore } from "../stores/appStore"
import { SESSIONS_KEY, upsertSessionInCache } from "./useSessions"
import { useQueryClient } from "@tanstack/react-query"

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
  /** From `useSessions().newAgent` — single source of draft-reuse logic. */
  newAgent: (cwd?: string) => Promise<unknown>
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
  onOpenBugReport: () => void
}

/**
 * Shared File/Edit/View/Help actions for in-window menus and the native macOS
 * menu bar. Handler object is built once (refs) so chat/tab switches do not
 * thrash menu children.
 */
export const useTitleBarActions = ({
  newAgent,
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
    newAgent,
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport,
    isBootstrapped,
    queryClient,
  })
  optsRef.current = {
    newAgent,
    onOpenCommandPalette,
    onOpenSearch,
    onOpenBugReport,
    isBootstrapped,
    queryClient,
  }

  const handlersRef = useRef<TitleBarActionHandlers | null>(null)
  if (!handlersRef.current) {
    handlersRef.current = {
      newAgent: () => {
        if (!optsRef.current.isBootstrapped) return
        void optsRef.current.newAgent().catch((err: unknown) => {
          useAppStore
            .getState()
            .pushToast(`Could not create agent: ${toInvokeError(err)}`, "error")
        })
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
            upsertSessionInCache(optsRef.current.queryClient, meta)
            state.setActiveSessionId(meta.id, { panel: "closed" })
            state.setRoute("chat")
            void optsRef.current.queryClient.invalidateQueries({
              queryKey: SESSIONS_KEY,
            })
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
