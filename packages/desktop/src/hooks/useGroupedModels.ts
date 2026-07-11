import { useMemo } from "react"
import type { BuiltinProvider, ModelInfoDto } from "../lib/types"

const capitalize = (s: string): string =>
  s.length === 0 ? s : s.charAt(0).toUpperCase() + s.slice(1)

export type ModelGroup = {
  providerId: string
  label: string
  items: ModelInfoDto[]
}

/** Shared provider-grouping + search-filter logic behind both `ModelPicker`
 * (composer, compact pill trigger) and `ModelSelect` (settings, form-field
 * trigger). Groups preserve each group's first-seen order (the models
 * list's own ordering) rather than sorting — matches the reference design's
 * provider clusters without re-ranking providers. */
export const useGroupedModels = (
  models: ModelInfoDto[],
  query: string,
  builtinProviders: BuiltinProvider[] = [],
): { groups: ModelGroup[]; providerLabel: (providerId: string) => string } => {
  const providerLabel = useMemo(() => {
    const byId = new Map(builtinProviders.map((p) => [p.id, p.label]))
    return (providerId: string) => byId.get(providerId) ?? capitalize(providerId)
  }, [builtinProviders])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return models
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        (m.displayName?.toLowerCase().includes(q) ?? false) ||
        m.providerId.toLowerCase().includes(q) ||
        providerLabel(m.providerId).toLowerCase().includes(q),
    )
  }, [models, query, providerLabel])

  const groups = useMemo(() => {
    const order: string[] = []
    const byProvider = new Map<string, ModelInfoDto[]>()
    for (const m of filtered) {
      if (!byProvider.has(m.providerId)) {
        byProvider.set(m.providerId, [])
        order.push(m.providerId)
      }
      byProvider.get(m.providerId)?.push(m)
    }
    return order.map((providerId) => ({
      providerId,
      label: providerLabel(providerId),
      items: byProvider.get(providerId) ?? [],
    }))
  }, [filtered, providerLabel])

  return { groups, providerLabel }
}
