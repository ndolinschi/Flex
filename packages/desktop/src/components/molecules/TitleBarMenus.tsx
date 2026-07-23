import type { TitleBarActionHandlers } from "../../hooks/useTitleBarActions"
import { detectWindowHost } from "../../lib/windowChrome"
import { cn } from "../../lib/utils"
import {
  Menubar,
  MenubarContent,
  MenubarGroup,
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
  handlers: TitleBarActionHandlers
  isBootstrapped: boolean
  canSearch: boolean
  canCommandPalette: boolean
}

const isMac = () => detectWindowHost() === "macos"
const mod = () => (isMac() ? "⌘" : "Ctrl+")

/** Cursor-style File / Edit / View / Help menus in the custom title bar (Windows/Linux).
 * Uses shadcn Menubar so hover-open coordination between menus is automatic. */
export const TitleBarMenus = ({
  handlers,
  isBootstrapped,
  canSearch,
  canCommandPalette,
}: TitleBarMenusProps) => {
  const menus: Record<MenuId, { label: string; items: MenuItem[] }> = {
    file: {
      label: "File",
      items: [
        {
          id: "new-agent",
          label: "New Agent",
          hint: `${mod()}N`,
          disabled: !isBootstrapped,
          run: () => handlers.newAgent(),
        },
        {
          id: "open-folder",
          label: "Open Folder…",
          disabled: !isBootstrapped,
          run: () => handlers.openFolder(),
        },
        { id: "sep-1", label: "", separator: true },
        {
          id: "settings",
          label: "Settings…",
          disabled: !isBootstrapped,
          run: () => handlers.settings(),
        },
        { id: "sep-2", label: "", separator: true },
        {
          id: "quit",
          label: "Quit",
          hint: isMac() ? "⌘Q" : "Alt+F4",
          run: () => handlers.quit(),
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
          disabled: !canSearch,
          run: () => handlers.search(),
        },
        {
          id: "command-palette",
          label: "Command Palette…",
          hint: `${mod()}⇧P`,
          disabled: !canCommandPalette,
          run: () => handlers.commandPalette(),
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
          run: () => handlers.toggleSidebar(),
        },
        {
          id: "toggle-panel",
          label: "Toggle Panel",
          hint: `${mod()}J`,
          disabled: !isBootstrapped,
          run: () => handlers.togglePanel(),
        },
        { id: "sep-v", label: "", separator: true },
        {
          id: "toggle-theme",
          label: "Toggle Theme",
          run: () => handlers.toggleTheme(),
        },
      ],
    },
    help: {
      label: "Help",
      items: [
        {
          id: "docs",
          label: "Documentation",
          run: () => handlers.docs(),
        },
        {
          id: "submit-bug",
          label: "Submit Bug…",
          run: () => handlers.submitBug(),
        },
        {
          id: "issues",
          label: "Open Issues on GitHub",
          run: () => handlers.issues(),
        },
      ],
    },
  }

  return (
    <Menubar
      className={cn(
        "h-full items-center gap-px border-0 px-0.5 py-0",
        "rounded-none bg-transparent p-0",
      )}
    >
      {(Object.keys(menus) as MenuId[]).map((id) => {
        const menu = menus[id]
        return (
          <MenubarMenu key={id}>
            <MenubarTrigger
              className={cn(
                "h-[22px] rounded-sm px-1.5 py-0 text-xs font-normal leading-none",
                "text-ink-muted",
                "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                "hover:bg-fill-4 hover:text-ink",
                "aria-expanded:bg-fill-4 aria-expanded:text-ink",
              )}
            >
              {menu.label}
            </MenubarTrigger>
            <MenubarContent align="start" sideOffset={2} className="min-w-[188px]">
              <MenubarGroup>
                {menu.items.map((item) =>
                  item.separator ? (
                    <MenubarSeparator key={item.id} />
                  ) : (
                    <MenubarItem
                      key={item.id}
                      disabled={item.disabled}
                      onClick={() => {
                        if (item.disabled || !item.run) return
                        item.run()
                      }}
                    >
                      {item.label}
                      {item.hint ? (
                        <MenubarShortcut>{item.hint}</MenubarShortcut>
                      ) : null}
                    </MenubarItem>
                  ),
                )}
              </MenubarGroup>
            </MenubarContent>
          </MenubarMenu>
        )
      })}
    </Menubar>
  )
}

export const AppMark = ({ className }: { className?: string }) => (
  <svg
    viewBox="0 0 16 16"
    fill="none"
    className={cn("size-3.5 shrink-0 text-ink-secondary", className)}
    aria-hidden
  >
    <path
      d="M4 2.5h8M4 2.5v11M4 8h5.5"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
)
