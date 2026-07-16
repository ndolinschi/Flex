import type { UiMentionHit, UiMentionProvider, UiPlugin, UiPluginTab } from "./types"

const plugins = new Map<string, UiPlugin>()

/** Register (or replace) a UI plugin. Idempotent by `id`. */
export const registerUiPlugin = (plugin: UiPlugin): void => {
  plugins.set(plugin.id, plugin)
}

export const unregisterUiPlugin = (id: string): void => {
  plugins.delete(id)
}

export const listUiPlugins = (): UiPlugin[] => [...plugins.values()]

/** Enabled right-panel tabs contributed by registered plugins. */
export const pluginRightPanelTabs = (): UiPluginTab[] =>
  listUiPlugins().flatMap((p) =>
    (p.tabs ?? []).filter((t) => t.enabled !== false),
  )

export const findPluginTab = (id: string): UiPluginTab | undefined =>
  pluginRightPanelTabs().find((t) => t.id === id)

export const pluginMentionProviders = (): UiMentionProvider[] =>
  listUiPlugins().flatMap((p) => p.mentionProviders ?? [])

/** True when any registered plugin opts into inline prompt completion. */
export const hasInlineCompletionPlugin = (): boolean =>
  listUiPlugins().some((p) => p.inlineCompletion === true)

/** Fan-out @-mention search across every registered provider. */
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

/** Test helper — wipe the registry between vitest cases. */
export const resetUiPluginsForTests = (): void => {
  plugins.clear()
}
