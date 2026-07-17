import { useEffect, useRef } from "react"
import { Menu } from "@tauri-apps/api/menu"
import { MenuItem } from "@tauri-apps/api/menu/menuItem"
import { PredefinedMenuItem } from "@tauri-apps/api/menu/predefinedMenuItem"
import { Submenu } from "@tauri-apps/api/menu/submenu"
import { isBrowserPreview } from "../lib/browserPreview"
import type { TitleBarActionHandlers } from "./useTitleBarActions"

const APP_NAME = "Flex"

type UseNativeAppMenuOpts = {
  /** Install the macOS menu bar when true (macOS host only). */
  enabled: boolean
  isBootstrapped: boolean
  canSearch: boolean
  canCommandPalette: boolean
  handlers: TitleBarActionHandlers
}

/**
 * Installs a native macOS application menu (File / Edit / View / Help plus
 * the standard app submenu). No-ops outside Tauri or when `enabled` is false.
 */
export const useNativeAppMenu = ({
  enabled,
  isBootstrapped,
  canSearch,
  canCommandPalette,
  handlers,
}: UseNativeAppMenuOpts): void => {
  // Keep the latest handlers without rebuilding the menu every render.
  const handlersRef = useRef(handlers)
  handlersRef.current = handlers

  useEffect(() => {
    if (!enabled || isBrowserPreview()) return

    let cancelled = false

    const install = async () => {
      const h = () => handlersRef.current

      try {
        const appSubmenu = await Submenu.new({
          text: APP_NAME,
          items: [
            await PredefinedMenuItem.new({
              item: { About: { name: APP_NAME } },
            }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await MenuItem.new({
              id: "settings",
              text: "Settings…",
              accelerator: "CmdOrCtrl+,",
              enabled: isBootstrapped,
              action: () => h().settings(),
            }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await PredefinedMenuItem.new({ item: "Services" }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await PredefinedMenuItem.new({ item: "Hide" }),
            await PredefinedMenuItem.new({ item: "HideOthers" }),
            await PredefinedMenuItem.new({ item: "ShowAll" }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await PredefinedMenuItem.new({ item: "Quit" }),
          ],
        })

        const fileSubmenu = await Submenu.new({
          text: "File",
          items: [
            await MenuItem.new({
              id: "new-agent",
              text: "New Agent",
              accelerator: "CmdOrCtrl+N",
              enabled: isBootstrapped,
              action: () => h().newAgent(),
            }),
            await MenuItem.new({
              id: "open-folder",
              text: "Open Folder…",
              enabled: isBootstrapped,
              action: () => h().openFolder(),
            }),
          ],
        })

        const editSubmenu = await Submenu.new({
          text: "Edit",
          items: [
            await PredefinedMenuItem.new({ item: "Undo" }),
            await PredefinedMenuItem.new({ item: "Redo" }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await PredefinedMenuItem.new({ item: "Cut" }),
            await PredefinedMenuItem.new({ item: "Copy" }),
            await PredefinedMenuItem.new({ item: "Paste" }),
            await PredefinedMenuItem.new({ item: "SelectAll" }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await MenuItem.new({
              id: "search",
              text: "Search Agents…",
              accelerator: "CmdOrCtrl+K",
              enabled: canSearch,
              action: () => h().search(),
            }),
            await MenuItem.new({
              id: "command-palette",
              text: "Command Palette…",
              accelerator: "CmdOrCtrl+Shift+P",
              enabled: canCommandPalette,
              action: () => h().commandPalette(),
            }),
          ],
        })

        const viewSubmenu = await Submenu.new({
          text: "View",
          items: [
            await MenuItem.new({
              id: "toggle-sidebar",
              text: "Toggle Sidebar",
              accelerator: "CmdOrCtrl+B",
              enabled: isBootstrapped,
              action: () => h().toggleSidebar(),
            }),
            await MenuItem.new({
              id: "toggle-panel",
              text: "Toggle Panel",
              accelerator: "CmdOrCtrl+J",
              enabled: isBootstrapped,
              action: () => h().togglePanel(),
            }),
            await PredefinedMenuItem.new({ item: "Separator" }),
            await MenuItem.new({
              id: "toggle-theme",
              text: "Toggle Theme",
              action: () => h().toggleTheme(),
            }),
          ],
        })

        const windowSubmenu = await Submenu.new({
          text: "Window",
          items: [
            await PredefinedMenuItem.new({ item: "Minimize" }),
            await PredefinedMenuItem.new({ item: "Fullscreen" }),
          ],
        })
        await windowSubmenu.setAsWindowsMenuForNSApp()

        const helpSubmenu = await Submenu.new({
          text: "Help",
          items: [
            await MenuItem.new({
              id: "docs",
              text: "Documentation",
              action: () => h().docs(),
            }),
            await MenuItem.new({
              id: "submit-bug",
              text: "Submit Bug…",
              action: () => h().submitBug(),
            }),
            await MenuItem.new({
              id: "issues",
              text: "Open Issues on GitHub",
              action: () => h().issues(),
            }),
          ],
        })
        await helpSubmenu.setAsHelpMenuForNSApp()

        const menu = await Menu.new({
          items: [
            appSubmenu,
            fileSubmenu,
            editSubmenu,
            viewSubmenu,
            windowSubmenu,
            helpSubmenu,
          ],
        })

        if (cancelled) return
        await menu.setAsAppMenu()
      } catch {
        // Browser preview / missing ACL / older runtime — leave default menu.
      }
    }

    void install()
    return () => {
      cancelled = true
    }
  }, [enabled, isBootstrapped, canSearch, canCommandPalette])
}
