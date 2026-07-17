import { useEffect, useRef, useState } from "react"
import type { TitleBarActionHandlers } from "../../hooks/useTitleBarActions"
import { detectWindowHost } from "../../lib/windowChrome"
import { cn } from "../../lib/utils"
import { PopoverItem, PopoverTray } from "./PopoverTray"

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

/** Cursor-style File / Edit / View / Help menus in the custom title bar (Windows/Linux). */
export const TitleBarMenus = ({
  handlers,
  isBootstrapped,
  canSearch,
  canCommandPalette,
}: TitleBarMenusProps) => {
  const [openMenu, setOpenMenu] = useState<MenuId | null>(null)
  const rootRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!openMenu) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpenMenu(null)
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [openMenu])

  const close = () => setOpenMenu(null)

  const run = (fn: () => void) => {
    close()
    fn()
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
    <div ref={rootRef} className="flex h-full items-center gap-px px-0.5">
      {(Object.keys(menus) as MenuId[]).map((id) => {
        const menu = menus[id]
        const open = openMenu === id
        return (
          <div key={id} className="relative flex h-full items-center">
            <button
              type="button"
              aria-haspopup="menu"
              aria-expanded={open}
              onClick={() => setOpenMenu(open ? null : id)}
              onMouseEnter={() => {
                if (openMenu && openMenu !== id) setOpenMenu(id)
              }}
              className={cn(
                "flex h-[22px] items-center rounded-sm px-1.5 text-xs leading-none text-ink-secondary",
                "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
                "hover:bg-fill-3 hover:text-ink",
                open && "bg-fill-3 text-ink",
              )}
            >
              {menu.label}
            </button>
            <PopoverTray
              open={open}
              onClose={close}
              placement="below"
              role="menu"
              aria-label={menu.label}
              className="left-0 top-full mt-0.5 min-w-[188px] py-1"
            >
              {menu.items.map((item) =>
                item.separator ? (
                  <div
                    key={item.id}
                    role="separator"
                    className="my-1 h-px bg-stroke-3"
                  />
                ) : (
                  <MenuRow
                    key={item.id}
                    label={item.label}
                    hint={item.hint}
                    disabled={item.disabled}
                    onClick={() => {
                      if (item.disabled || !item.run) return
                      run(item.run)
                    }}
                  />
                ),
              )}
            </PopoverTray>
          </div>
        )
      })}
    </div>
  )
}

const MenuRow = ({
  label,
  hint,
  disabled,
  onClick,
}: {
  label: string
  hint?: string
  disabled?: boolean
  onClick: () => void
}) => (
  <PopoverItem
    role="menuitem"
    disabled={disabled}
    onClick={onClick}
    className="justify-between gap-6 px-3 py-1.5 text-sm"
  >
    <span>{label}</span>
    {hint ? (
      <span className="text-xs text-ink-muted [font-variant-numeric:tabular-nums]">
        {hint}
      </span>
    ) : null}
  </PopoverItem>
)

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
