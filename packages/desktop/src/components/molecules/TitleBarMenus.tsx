import { useState } from "react"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { openUrl } from "@tauri-apps/plugin-opener"
import { useSessions } from "../../hooks/useSessions"
import { closeWindow, detectWindowHost } from "../../lib/windowChrome"
import { newAgentCreateInput } from "../../lib/sessions"
import { createSession, toInvokeError } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { BugReportDialog } from "./BugReportDialog"
import {
  Menubar,
  MenubarContent,
  MenubarItem,
  MenubarMenu,
  MenubarSeparator,
  MenubarShortcut,
  MenubarTrigger,
} from "@/components/ui/menubar"

type MenuId = "file" | "edit" | "view" | "help"

type MenuItem = {
  id: string
  label: string
  hint?: string
  disabled?: boolean
  separator?: boolean
  run?: () => void
}

type TitleBarMenusProps = {
  onOpenCommandPalette?: () => void
  onOpenSearch?: () => void
}

const isMac = () => detectWindowHost() === "macos"
const mod = () => (isMac() ? "⌘" : "Ctrl+")

/** Cursor-style File / Edit / View / Help menus in the custom title bar. */
export const TitleBarMenus = ({
  onOpenCommandPalette,
  onOpenSearch,
}: TitleBarMenusProps) => {
  const [bugOpen, setBugOpen] = useState(false)
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

  const menus: Record<MenuId, { label: string; items: MenuItem[] }> = {
    file: {
      label: "File",
      items: [
        {
          id: "new-agent",
          label: "New Agent",
          hint: `${mod()}N`,
          disabled: !isBootstrapped,
          run: () => void newAgent(),
        },
        {
          id: "open-folder",
          label: "Open Folder…",
          disabled: !isBootstrapped,
          run: () => void openFolder(),
        },
        { id: "sep-1", label: "", separator: true },
        {
          id: "settings",
          label: "Settings…",
          disabled: !isBootstrapped,
          run: () => setRoute("settings"),
        },
        { id: "sep-2", label: "", separator: true },
        {
          id: "quit",
          label: "Quit",
          hint: isMac() ? "⌘Q" : "Alt+F4",
          run: () => void closeWindow(),
        },
      ],
    },
    edit: {
      label: "Edit",
      items: [
        {
          id: "search",
          label: "Search Agents…",
          hint: `${mod()}K`,
          disabled: !onOpenSearch,
          run: () => onOpenSearch?.(),
        },
        {
          id: "command-palette",
          label: "Command Palette…",
          hint: `${mod()}⇧P`,
          disabled: !onOpenCommandPalette,
          run: () => onOpenCommandPalette?.(),
        },
      ],
    },
    view: {
      label: "View",
      items: [
        {
          id: "toggle-sidebar",
          label: "Toggle Sidebar",
          hint: `${mod()}B`,
          disabled: !isBootstrapped,
          run: () => toggleSidebarCollapsed(),
        },
        {
          id: "toggle-panel",
          label: "Toggle Panel",
          hint: `${mod()}J`,
          disabled: !isBootstrapped,
          run: () => toggleRightPanel(),
        },
        { id: "sep-v", label: "", separator: true },
        {
          id: "toggle-theme",
          label: "Toggle Theme",
          run: () => toggleTheme(),
        },
      ],
    },
    help: {
      label: "Help",
      items: [
        {
          id: "docs",
          label: "Documentation",
          run: () =>
            void openUrl("https://github.com/ndolinschi/Flex#readme").catch(
              () => undefined,
            ),
        },
        {
          id: "submit-bug",
          label: "Submit Bug…",
          run: () => setBugOpen(true),
        },
        {
          id: "issues",
          label: "Open Issues on GitHub",
          run: () =>
            void openUrl("https://github.com/ndolinschi/Flex/issues").catch(
              () => undefined,
            ),
        },
      ],
    },
  }

  return (
    <>
      <Menubar
        className={cn(
          "h-full gap-px rounded-none border-0 bg-transparent p-0 shadow-none",
        )}
      >
        {(Object.keys(menus) as MenuId[]).map((id) => {
          const menu = menus[id]
          return (
            <MenubarMenu key={id}>
              <MenubarTrigger
                className={cn(
                  "h-[22px] rounded-sm px-1.5 text-xs font-normal leading-none text-ink-secondary",
                  "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                )}
              >
                {menu.label}
              </MenubarTrigger>
              <MenubarContent
                align="start"
                sideOffset={4}
                className="min-w-[188px] py-1"
              >
                {menu.items.map((item) =>
                  item.separator ? (
                    <MenubarSeparator
                      key={item.id}
                      className="mx-1 my-1 bg-stroke-3"
                    />
                  ) : (
                    <MenubarItem
                      key={item.id}
                      disabled={item.disabled}
                      className="justify-between gap-6 px-3 py-1.5 text-sm"
                      onSelect={() => {
                        if (item.disabled || !item.run) return
                        item.run()
                      }}
                    >
                      <span>{item.label}</span>
                      {item.hint ? (
                        <MenubarShortcut>{item.hint}</MenubarShortcut>
                      ) : null}
                    </MenubarItem>
                  ),
                )}
              </MenubarContent>
            </MenubarMenu>
          )
        })}
      </Menubar>
      <BugReportDialog open={bugOpen} onClose={() => setBugOpen(false)} />
    </>
  )
}

/** Compact app mark used on Windows/Linux title bars (Cursor-style). */
export const AppMark = ({ className }: { className?: string }) => (
  <div
    className={cn(
      "flex h-full items-center justify-center px-1.5 text-ink-secondary",
      className,
    )}
    aria-hidden
  >
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path
        d="M8 1.5L14 5v6L8 14.5 2 11V5L8 1.5Z"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinejoin="round"
      />
      <path
        d="M8 1.5V8m0 0l6-3M8 8l-6-3M8 8v6.5"
        stroke="currentColor"
        strokeWidth="1.1"
        strokeLinejoin="round"
        opacity="0.7"
      />
    </svg>
  </div>
)
