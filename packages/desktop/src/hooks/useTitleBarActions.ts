import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { openUrl } from "@tauri-apps/plugin-opener"
import { useSessions } from "./useSessions"
import { closeWindow } from "../lib/windowChrome"
import { newAgentCreateInput } from "../lib/sessions"
import { createSession, toInvokeError } from "../lib/tauri"
import { useAppStore } from "../stores/appStore"

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

/** Shared File/Edit/View/Help actions for in-window menus and the native macOS menu bar. */
export const useTitleBarActions = ({
  onOpenCommandPalette,
  onOpenSearch,
  onOpenBugReport,
}: UseTitleBarActionsOpts): {
  isBootstrapped: boolean
  handlers: TitleBarActionHandlers
} => {
  const { newAgent } = useSessions()
  const setRoute = useAppStore((s) => s.setRoute)
  const toggleSidebarCollapsed = useAppStore((s) => s.toggleSidebarCollapsed)
  const toggleRightPanel = useAppStore((s) => s.toggleRightPanel)
  const toggleTheme = useAppStore((s) => s.toggleTheme)
  const pushRecentCwd = useAppStore((s) => s.pushRecentCwd)
  const pushToast = useAppStore((s) => s.pushToast)
  const setActiveSessionId = useAppStore((s) => s.setActiveSessionId)
  const isBootstrapped = useAppStore((s) => s.isBootstrapped)

  const openFolder = async () => {
    if (!isBootstrapped) return
    try {
      const path = await openDialog({ directory: true, multiple: false })
      if (!path || Array.isArray(path)) return
      pushRecentCwd(path)
      const meta = await createSession(newAgentCreateInput(path))
      setActiveSessionId(meta.id, { panel: "closed" })
      setRoute("chat")
    } catch (err) {
      pushToast(`Could not open folder: ${toInvokeError(err)}`, "error")
    }
  }

  return {
    isBootstrapped,
    handlers: {
      newAgent: () => {
        if (!isBootstrapped) return
        void newAgent()
      },
      openFolder: () => {
        void openFolder()
      },
      settings: () => {
        if (!isBootstrapped) return
        setRoute("settings")
      },
      quit: () => {
        void closeWindow()
      },
      search: () => {
        onOpenSearch?.()
      },
      commandPalette: () => {
        onOpenCommandPalette?.()
      },
      toggleSidebar: () => {
        if (!isBootstrapped) return
        toggleSidebarCollapsed()
      },
      togglePanel: () => {
        if (!isBootstrapped) return
        toggleRightPanel()
      },
      toggleTheme: () => {
        toggleTheme()
      },
      docs: () => {
        void openUrl(DOCS_URL).catch(() => undefined)
      },
      submitBug: () => {
        onOpenBugReport()
      },
      issues: () => {
        void openUrl(ISSUES_URL).catch(() => undefined)
      },
    },
  }
}
