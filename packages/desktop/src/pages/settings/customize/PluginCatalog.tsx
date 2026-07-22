import { useMemo, useState } from "react"
import {
  BookOpen,
  Check,
  Globe,
  Monitor,
  ShieldCheck,
  Search,
  AppWindow,
  MessageSquare,
  Users,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner as ButtonSpinner } from "@/components/ui/spinner"
import { Spinner } from "../../../components/atoms"
import { Switch } from "@/components/ui/switch"
import { ErrorBanner, SettingsSection } from "../../../components/molecules"
import { useProviderConfig } from "../../../hooks/useProviderConfig"
import type { PluginPrefs } from "../../../lib/types"
import { cn } from "../../../lib/utils"
import { Input } from "@/components/ui/input"

type PluginKey = Exclude<
  keyof PluginPrefs,
  | "autoContext"
  | "autoUpdateIndex"
  | "learningRequireHumanApproval"
  | "learningRequireVerifiedMemory"
  | "autoMode"
  | "autoModeRouterModel"
  | "autoCompact"
  | "autoCompactThresholdPercent"
  | "compactionMode"
  | "modeSwitchVetoMs"
  | "delegationRules"
>

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
    key: "index",
    name: "Code index",
    description: "Agentic code search: SearchCode / FindSymbol / RepoMap over a local index.",
    icon: Search,
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
  {
    key: "messaging",
    name: "Agent messaging",
    description:
      "Peer coordination: GetActiveAgents / SendMessage / GetMessages / SwitchMode — lets agents communicate and propose mode changes.",
    icon: MessageSquare,
    category: "Coordination",
  },
  {
    key: "council",
    name: "Council",
    description:
      "Second-opinion verification on another model — enables the Verifier plugin automatically.",
    icon: Users,
    category: "Coordination",
  },
  {
    key: "browser",
    name: "Browser",
    description:
      "Drive the embedded Browser tab: navigate, screenshot, eval, click, console, DevTools.",
    icon: AppWindow,
    category: "Desktop plugins",
  },
  {
    key: "computer",
    name: "Computer use",
    description:
      "ChatGPT-style desktop control: screenshot, animated agent cursor, click, type, open apps.",
    icon: Monitor,
    category: "Desktop plugins",
  },
]

/** Searchable plugin toggle cards backed by the provider config's fixed
 * `PluginPrefs` shape. Rendered as a list (one row per plugin), matching
 * the MCP catalog/servers lists below it in `CustomizeSection`. */
export const PluginCatalog = () => {
  const { config, isLoading, save } = useProviderConfig()
  const [query, setQuery] = useState("")
  const [busyKey, setBusyKey] = useState<string | null>(null)
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

  const savePlugins = async (
    patch: Partial<PluginPrefs>,
    busyId: string,
  ) => {
    if (!config || !plugins || busyKey) return
    setError(null)
    setBusyKey(busyId)
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
        plugins: {
          ...plugins,
          autoContext: plugins.autoContext ?? false,
          autoUpdateIndex: plugins.autoUpdateIndex ?? false,
          learningRequireHumanApproval:
            plugins.learningRequireHumanApproval ?? false,
          learningRequireVerifiedMemory:
            plugins.learningRequireVerifiedMemory ?? false,
          browser: plugins.browser ?? false,
          computer: plugins.computer ?? false,
          messaging: plugins.messaging ?? false,
          council: plugins.council ?? false,
          autoMode: plugins.autoMode ?? false,
          autoCompact: plugins.autoCompact ?? true,
          autoCompactThresholdPercent: plugins.autoCompactThresholdPercent ?? 85,
          compactionMode: plugins.compactionMode ?? "standard",
          modeSwitchVetoMs: plugins.modeSwitchVetoMs ?? 2000,
          delegationRules: plugins.delegationRules ?? "",
          ...patch,
        },
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyKey(null)
    }
  }

  const handleToggle = async (key: PluginKey) => {
    if (!plugins) return
    await savePlugins({ [key]: !plugins[key] }, key)
  }

  const engineVisible = useMemo(
    () => visible.filter((p) => p.category === "Engine plugins"),
    [visible],
  )
  const coordinationVisible = useMemo(
    () => visible.filter((p) => p.category === "Coordination"),
    [visible],
  )
  const desktopVisible = useMemo(
    () => visible.filter((p) => p.category === "Desktop plugins"),
    [visible],
  )

  const searchInput = (
    <Input
      value={query}
      onChange={(e) => setQuery(e.target.value)}
      placeholder="Search plugins…"
      aria-label="Search plugins"
      className="w-56"
    />
  )

  const renderPluginRow = (plugin: PluginCardSpec) => {
    if (!plugins) return null
    const Icon = plugin.icon
    const added = plugins[plugin.key]
    const busy = busyKey === plugin.key
    return (
      <div key={plugin.key} className="flex flex-col gap-2 px-3.5 py-3">
        <div className="flex items-center gap-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-fill-3">
            <Icon className="h-4 w-4 text-icon-2" aria-hidden />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-base text-ink-secondary">{plugin.name}</p>
            <p className="mt-0.5 truncate text-base text-ink-muted">
              {plugin.description}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant={added ? "ghost" : "secondary"}
              size="sm"
              disabled={busy || (busyKey !== null && !busy)}
              onClick={() => void handleToggle(plugin.key)}
              className={cn("shrink-0", added && "text-green")}
            >
              {busy ? <ButtonSpinner data-icon="inline-start" /> : null}
              {added ? (
                <>
                  <Check className="h-3 w-3" aria-hidden /> Added
                </>
              ) : (
                "Add"
              )}
            </Button>
          </div>
        </div>

        {plugin.key === "learning" && added ? (
          <div className="ml-11 flex flex-col gap-2 border-l border-stroke-3 pl-3">
            <label
              htmlFor="learning-require-human-approval"
              className="flex items-center justify-between gap-3 text-sm text-ink-secondary"
            >
              <span>Require human approval for SkillSave / MemoryWrite</span>
              <Switch
                id="learning-require-human-approval"
                checked={!!plugins.learningRequireHumanApproval}
                onCheckedChange={(on) =>
                  void savePlugins(
                    { learningRequireHumanApproval: on },
                    "learningRequireHumanApproval",
                  )
                }
                aria-label="Require human approval for SkillSave / MemoryWrite"
                disabled={busyKey !== null}
              />
            </label>
            <label
              htmlFor="learning-require-verified-memory"
              className="flex items-center justify-between gap-3 text-sm text-ink-secondary"
            >
              <span>Require Verify before memory writes</span>
              <Switch
                id="learning-require-verified-memory"
                checked={!!plugins.learningRequireVerifiedMemory}
                onCheckedChange={(on) =>
                  void savePlugins(
                    { learningRequireVerifiedMemory: on },
                    "learningRequireVerifiedMemory",
                  )
                }
                aria-label="Require Verify before memory writes"
                disabled={busyKey !== null}
              />
            </label>
            {plugins.learningRequireVerifiedMemory && !plugins.verifier ? (
              <p className="text-xs text-ink-faint">
                Add Verifier too — otherwise SkillSave / MemoryWrite stay
                blocked.
              </p>
            ) : null}
          </div>
        ) : null}

        {plugin.key === "verifier" && added ? (
          <p className="ml-11 text-xs text-ink-faint">
            Adds Verify / SubmitVerdict. Restart sessions after enabling.
          </p>
        ) : null}

        {plugin.key === "messaging" && added ? (
          <p className="ml-11 text-xs text-ink-faint">
            Agents can coordinate on shared files. SwitchMode shows a veto countdown in the composer.
            Restart sessions after enabling.
          </p>
        ) : null}

        {plugin.key === "council" && added ? (
          <p className="ml-11 text-xs text-ink-faint">
            Enables the Verifier automatically. Verify tool routes to a separate model for grading.
            Restart sessions after enabling.
          </p>
        ) : null}

        {plugin.key === "browser" && added ? (
          <p className="ml-11 text-xs text-ink-faint">
            Needs an open Browser tab. Screenshots work on macOS, Linux (grim /
            ImageMagick), and Windows. Rebuild / new session after enabling.
          </p>
        ) : null}

        {plugin.key === "computer" && added ? (
          <p className="ml-11 text-xs text-ink-faint">
            Screenshots + mouse/keyboard on macOS (Accessibility), Linux
            (xdotool/grim), and Windows. Click / type / open always ask.
            Rebuild / new session after enabling.
          </p>
        ) : null}
      </div>
    )
  }

  const loadingOrEmpty = isLoading || !plugins
  const showEngine =
    loadingOrEmpty || visible.length === 0 || engineVisible.length > 0
  const showCoordination = !loadingOrEmpty && coordinationVisible.length > 0
  const showDesktop = !loadingOrEmpty && desktopVisible.length > 0

  return (
    <div className="flex flex-col gap-3">
      {error ? <ErrorBanner message={error} onDismiss={() => setError(null)} /> : null}

      {showEngine ? (
        <SettingsSection
          title="Engine plugins"
          description="Native tool bundles the engine can load into a session."
          rowId="tools-plugins"
          actions={searchInput}
          className="mb-0"
        >
          {loadingOrEmpty ? (
            <div className="flex items-center gap-2 px-3.5 py-3 text-sm text-ink-muted">
              <Spinner size="sm" /> Loading configuration…
            </div>
          ) : visible.length === 0 ? (
            <p className="px-4 py-8 text-center text-sm text-ink-muted">
              No plugins match “{query}”.
            </p>
          ) : (
            engineVisible.map(renderPluginRow)
          )}
        </SettingsSection>
      ) : null}

      {showCoordination ? (
        <SettingsSection
          title="Coordination"
          description="Multi-agent coordination: peer messaging, mode switching, and council review."
          rowId="tools-coordination-plugins"
          className="mb-0"
        >
          {coordinationVisible.map(renderPluginRow)}
        </SettingsSection>
      ) : null}

      {showDesktop ? (
        <SettingsSection
          title="Desktop plugins"
          description="Tools that drive the embedded Browser panel or the host OS. Desktop app only."
          rowId="tools-desktop-plugins"
          actions={showEngine ? undefined : searchInput}
          className="mb-0"
        >
          {desktopVisible.map(renderPluginRow)}
        </SettingsSection>
      ) : null}
    </div>
  )
}
