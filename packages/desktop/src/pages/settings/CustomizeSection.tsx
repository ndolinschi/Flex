import { McpCatalogSection } from "./customize/McpCatalogSection"
import { McpServersSection } from "./customize/McpServersSection"
import { PluginCatalog } from "./customize/PluginCatalog"
import { InlineCompletionSettingsCard } from "../../plugins/prompt-completion"

export const CustomizeContent = () => {
  return (
    <div className="flex flex-col gap-3">
      <PluginCatalog />
      <InlineCompletionSettingsCard />
      <McpCatalogSection />
      <McpServersSection />
    </div>
  )
}
