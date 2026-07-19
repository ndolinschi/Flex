import { useMemo } from "react"
import type { BuiltinProvider, ModelInfoDto } from "../lib/types"

const capitalize = (s: string): string =>
  s.length === 0 ? s : s.charAt(0).toUpperCase() + s.slice(1)

export type ModelGroup = {
  providerId: string
  label: string
  items: ModelInfoDto[]
}

const EMPTY_GROUPS: ModelGroup[] = []

/** Cap total rows rendered in searchable model menus. Large catalogs
 * (OpenRouter-style) otherwise mount hundreds of menu items per open. */
export const MODEL_MENU_VISIBLE_CAP = 80

/** Shared provider-grouping + search-filter logic behind both `ModelPicker`
 * (composer) and `ModelSelect` / plan toolbar pills. Groups preserve each
 * group's first-seen order (the models list's own ordering).
 *
 * Pass `enabled: false` while the menu is closed so composers/settings don't
 * rebuild group trees on every parent re-render. */
export const useGroupedModels = (
  models: ModelInfoDto[],
  query: string,
  builtinProviders: BuiltinProvider[] = [],
  enabled = true,
): {
  groups: ModelGroup[]
  totalMatched: number
  truncated: boolean
  providerLabel: (providerId: string) => string
} => {
  const providerLabel = useMemo(() => {
    const byId = new Map(builtinProviders.map((p) => [p.id, p.label]))
    return (providerId: string) => byId.get(providerId) ?? capitalize(providerId)
  }, [builtinProviders])

  const filtered = useMemo(() => {
    if (!enabled) return null
    const q = query.trim().toLowerCase()
    if (!q) return models
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        (m.displayName?.toLowerCase().includes(q) ?? false) ||
        m.providerId.toLowerCase().includes(q) ||
        providerLabel(m.providerId).toLowerCase().includes(q),
    )
  }, [enabled, models, query, providerLabel])

  const { groups, totalMatched, truncated } = useMemo(() => {
    if (!filtered) {
      return { groups: EMPTY_GROUPS, totalMatched: 0, truncated: false }
    }
    const order: string[] = []
    const byProvider = new Map<string, ModelInfoDto[]>()
    for (const m of filtered) {
      if (!byProvider.has(m.providerId)) {
        byProvider.set(m.providerId, [])
        order.push(m.providerId)
      }
      byProvider.get(m.providerId)?.push(m)
    }

    let remaining = MODEL_MENU_VISIBLE_CAP
    const capped: ModelGroup[] = []
    for (const providerId of order) {
      if (remaining <= 0) break
      const items = byProvider.get(providerId) ?? []
      const slice = items.slice(0, remaining)
      remaining -= slice.length
      capped.push({
        providerId,
        label: providerLabel(providerId),
        items: slice,
      })
    }

    return {
      groups: capped,
      totalMatched: filtered.length,
      truncated: filtered.length > MODEL_MENU_VISIBLE_CAP,
    }
  }, [filtered, providerLabel])

  return { groups, totalMatched, truncated, providerLabel }
}
