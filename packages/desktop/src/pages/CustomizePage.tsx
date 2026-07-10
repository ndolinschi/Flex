import { useMemo, useState } from "react"
import { BookOpen, Check, Globe, ShieldCheck } from "lucide-react"
import { Button, Spinner, TextInput } from "../components/atoms"
import { ErrorBanner } from "../components/molecules"
import { SettingsShell } from "../components/templates"
import { useProviderConfig } from "../hooks/useProviderConfig"
import type { PluginPrefs } from "../lib/types"
import { cn } from "../lib/utils"

type PluginKey = keyof PluginPrefs

type PluginCardSpec = {
  key: PluginKey
  name: string
  description: string
  icon: typeof Globe
  category: string
}

/** Engine plugin catalog — mirrors the fixed PluginPrefs shape on the wire. */
const PLUGIN_CATALOG: PluginCardSpec[] = [
  {
    key: "search",
    name: "Search",
    description: "Web tools: search_web + scrape_page with a researcher role.",
    icon: Globe,
    category: "Engine plugins",
  },
  {
    key: "learning",
    name: "Learning",
    description: "Persistent memory and skills: SkillSave / MemoryWrite + reflection.",
    icon: BookOpen,
    category: "Engine plugins",
  },
  {
    key: "verifier",
    name: "Verifier",
    description: "Independent grading of results: Verify / SubmitVerdict tools.",
    icon: ShieldCheck,
    category: "Engine plugins",
  },
]

type CustomizePageProps = {
  embedded?: boolean
}

/** Cursor-style Customize view: searchable plugin cards with Add / Added. */
export const CustomizePage = ({ embedded = false }: CustomizePageProps) => {
  const { config, isLoading, save } = useProviderConfig()
  const [query, setQuery] = useState("")
  const [busyKey, setBusyKey] = useState<PluginKey | null>(null)
  const [error, setError] = useState<string | null>(null)

  const plugins = config?.plugins

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return PLUGIN_CATALOG
    return PLUGIN_CATALOG.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q),
    )
  }, [query])

  const handleToggle = async (key: PluginKey) => {
    if (!config || !plugins || busyKey) return
    setError(null)
    setBusyKey(key)
    try {
      // Round-trip every field: save_provider_config overwrites baseUrl and
      // defaultModel unconditionally, so a plugins-only payload would wipe them.
      await save({
        preferredProvider: config.preferredProvider ?? "",
        baseUrl: config.baseUrl,
        defaultModel: config.defaultModel,
        fallbackModels: config.fallbackModels,
        defaultIsolation:
          typeof config.defaultIsolation === "string"
            ? config.defaultIsolation
            : undefined,
        plugins: { ...plugins, [key]: !plugins[key] },
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyKey(null)
    }
  }

  const categories = [...new Set(visible.map((p) => p.category))]

  return (
    <SettingsShell title="Customize" wide embedded={embedded}>
      {error ? (
        <div className="mb-3">
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        </div>
      ) : null}

      <TextInput
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search plugins…"
        aria-label="Search plugins"
        className="mb-5"
      />

      {isLoading || !plugins ? (
        <div className="flex items-center gap-2 py-8 text-sm text-ink-muted">
          <Spinner size="sm" /> Loading configuration…
        </div>
      ) : visible.length === 0 ? (
        <p className="py-8 text-center text-sm text-ink-muted">
          No plugins match “{query}”.
        </p>
      ) : (
        categories.map((category) => (
          <section key={category} className="mb-6">
            <h2 className="mb-2 text-sm font-medium text-ink-secondary">
              {category}
            </h2>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {visible
                .filter((p) => p.category === category)
                .map((plugin) => {
                  const Icon = plugin.icon
                  const added = plugins[plugin.key]
                  const busy = busyKey === plugin.key
                  return (
                    <div
                      key={plugin.key}
                      className="flex items-start gap-3 rounded-lg border border-stroke-3 bg-panel p-3"
                    >
                      <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-fill-3">
                        <Icon className="h-4 w-4 text-icon-2" aria-hidden />
                      </div>
                      <div className="min-w-0 flex-1">
                        <p className="text-base text-ink">{plugin.name}</p>
                        <p className="mt-0.5 line-clamp-2 text-sm leading-normal text-ink-muted">
                          {plugin.description}
                        </p>
                      </div>
                      <Button
                        variant={added ? "ghost" : "secondary"}
                        size="sm"
                        isLoading={busy}
                        disabled={busyKey !== null && !busy}
                        onClick={() => void handleToggle(plugin.key)}
                        className={cn("shrink-0", added && "text-green")}
                      >
                        {added ? (
                          <>
                            <Check className="h-3 w-3" aria-hidden /> Added
                          </>
                        ) : (
                          "Add"
                        )}
                      </Button>
                    </div>
                  )
                })}
            </div>
          </section>
        ))
      )}
    </SettingsShell>
  )
}
