import type { UiMentionHit, UiMentionProvider, UiPlugin, UiPluginTab } from "./types"

const plugins = new Map<string, UiPlugin>()

export const registerUiPlugin = (plugin: UiPlugin): void => {
  plugins.set(plugin.id, plugin)
}

export const unregisterUiPlugin = (id: string): void => {
  plugins.delete(id)
}

export const listUiPlugins = (): UiPlugin[] => [...plugins.values()]

export const pluginRightPanelTabs = (): UiPluginTab[] =>
  listUiPlugins().flatMap((p) =>
    (p.tabs ?? []).filter((t) => t.enabled !== false),
  )

export const findPluginTab = (id: string): UiPluginTab | undefined =>
  pluginRightPanelTabs().find((t) => t.id === id)

export const pluginMentionProviders = (): UiMentionProvider[] =>
  listUiPlugins().flatMap((p) => p.mentionProviders ?? [])

export const hasInlineCompletionPlugin = (): boolean =>
  listUiPlugins().some((p) => p.inlineCompletion === true)

export const searchPluginMentions = async (
  query: string,
  cwd: string | undefined,
): Promise<UiMentionHit[]> => {
  const providers = pluginMentionProviders()
  if (providers.length === 0) return []
  const batches = await Promise.all(
    providers.map(async (p) => {
      try {
        return await p.search(query, cwd)
      } catch {
        return []
      }
    }),
  )
  return batches.flat()
}

export const resetUiPluginsForTests = (): void => {
  plugins.clear()
}
