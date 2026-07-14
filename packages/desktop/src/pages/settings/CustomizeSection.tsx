import { McpCatalogSection } from "./customize/McpCatalogSection"
import { McpServersSection } from "./customize/McpServersSection"
import { PluginCatalog } from "./customize/PluginCatalog"

/** Customize content: searchable plugin cards with Add / Added, plus the MCP
 * catalog and servers list. Mounted inside the Settings shell's "Tools & MCP"
 * section (DESIGN.md Settings) — the standalone
 * Customize route/page is gone; `App.tsx` now renders the unified settings
 * shell for all of settings/customize/automations/memory. No `SettingsShell`
 * wrapper here anymore since the shell itself owns nav+header.
 *
 * Sub-components live under `./customize/`; pure helpers (arg/env parsing,
 * the catalog install DTO assembler) live in `../../lib/mcp.ts`. */
export const CustomizeContent = () => {
  return (
    <div className="flex flex-col gap-3">
      <PluginCatalog />
      <McpCatalogSection />
      <McpServersSection />
    </div>
  )
}
