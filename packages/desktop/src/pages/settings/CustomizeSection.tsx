import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { McpCatalogSection } from "./customize/McpCatalogSection"
import { McpServersSection } from "./customize/McpServersSection"
import { PluginCatalog } from "./customize/PluginCatalog"
import { InlineCompletionSettingsCard } from "../../plugins/prompt-completion"

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
    <Accordion
      type="multiple"
      defaultValue={["plugins", "mcp"]}
      className="gap-3"
    >
      <AccordionItem value="plugins" className="border-0">
        <AccordionTrigger className="px-3.5 py-2 text-sm text-ink-secondary hover:no-underline">
          Plugins
        </AccordionTrigger>
        <AccordionContent className="flex flex-col gap-3 pb-0">
          <PluginCatalog />
          <InlineCompletionSettingsCard />
        </AccordionContent>
      </AccordionItem>
      <AccordionItem value="mcp" className="border-0">
        <AccordionTrigger className="px-3.5 py-2 text-sm text-ink-secondary hover:no-underline">
          MCP
        </AccordionTrigger>
        <AccordionContent className="flex flex-col gap-3 pb-0">
          <McpCatalogSection />
          <McpServersSection />
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  )
}
