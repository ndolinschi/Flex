import { Moon, Sun } from "lucide-react"
import { SettingsShell } from "../components/templates"
import { SettingsCard, SettingRow, SETTINGS_NAV_ITEMS, AccentColorPicker, ThemeLibrary } from "../components/molecules"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { ProviderSettingsForm } from "../components/organisms"
import { AUTOMATIONS_UI_ENABLED } from "../lib/featureFlags"
import { AutomationsContent } from "./settings/AutomationsSection"
import { CustomizeContent } from "./settings/CustomizeSection"
import { DiagnosticsContent } from "./settings/DiagnosticsSection"
import { IndexingContent } from "./settings/IndexingSection"
import { MemoryContent } from "./settings/MemorySection"
import { RemoteAccessContent } from "./settings/RemoteAccessSection"
import type { SettingsSectionId } from "../lib/settingsSearchIndex"
import type { PermissionMode, PluginPrefs } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import { useProviderConfig } from "../hooks/useProviderConfig"

const PERMISSION_MODE_OPTIONS: Array<{ value: PermissionMode; label: string }> = [
  { value: "default", label: "Ask (default)" },
  { value: "accept_edits", label: "Accept edits" },
  { value: "dont_ask", label: "Don't ask" },
  { value: "bypass_permissions", label: "Bypass all" },
]

const TITLES: Record<SettingsSectionId, string> = Object.fromEntries(
  SETTINGS_NAV_ITEMS.map((item) => [item.id, item.label]),
) as Record<SettingsSectionId, string>

const DESCRIPTIONS: Partial<Record<SettingsSectionId, string>> = {
  general: "App-wide preferences.",
  appearance: "Theme and display preferences.",
  models: "Configure the preferred native provider for the agent loop.",
  behavior: "Session defaults and where secrets are stored.",
  "remote-access":
    "Chat-only companion for a phone: see messages and send messages. No tools, MCP, or system control.",
  memory: "Durable notes the agent saves as it works.",
  indexing: "Local code index status, rebuild, auto-update, and auto-context.",
  "tools-mcp": "Native plugins and MCP servers the engine can load.",
  automations: "Run a prompt on a schedule or webhook.",
  diagnostics: "Debug logging, local crash capture, diagnostics export, and updates.",
}

/** General section — notification preferences (see DESIGN.md Settings).
 * "System notifications" gates the native OS notification
 * entirely (`notifyTurnCompleted`, background-session completions only);
 * "Completion sound" plays a short WebAudio chime on ANY turn completion
 * (active session included) when enabled — see `useGlobalSessionEvents`. */
const GeneralContent = () => {
  const notificationsEnabled = useAppStore((s) => s.notificationsEnabled)
  const setNotificationsEnabled = useAppStore((s) => s.setNotificationsEnabled)
  const completionSoundEnabled = useAppStore((s) => s.completionSoundEnabled)
  const setCompletionSoundEnabled = useAppStore((s) => s.setCompletionSoundEnabled)

  return (
    <SettingsCard label="Notifications">
      <SettingRow
        rowId="general-notifications"
        title="System notifications"
        description="Native OS notification when a background session finishes a turn. First enable triggers the OS permission prompt."
        first
      >
        <Switch
          checked={notificationsEnabled}
          onCheckedChange={setNotificationsEnabled}
          aria-label="Toggle system notifications"
          title="Toggle system notifications"
        />
      </SettingRow>
      <SettingRow
        rowId="general-completion-sound"
        title="Completion sound"
        description="Play a short chime whenever a turn finishes, including the session you're viewing."
      >
        <Switch
          checked={completionSoundEnabled}
          onCheckedChange={setCompletionSoundEnabled}
          aria-label="Toggle completion sound"
          title="Toggle completion sound"
        />
      </SettingRow>
    </SettingsCard>
  )
}

/** Appearance section — theme toggle moved here from the sidebar footer
 * icon-button (see DESIGN.md Settings). The sidebar's quick-access icon stays as a convenience
 * shortcut; this is now the canonical settings location. Accent color
 * (neutral by default, or a hue / custom hex) lives here too. */
const AppearanceContent = () => {
  const theme = useAppStore((s) => s.theme)
  const toggleTheme = useAppStore((s) => s.toggleTheme)

  return (
    <div className="flex flex-col gap-3">
      <SettingsCard label="Theme">
        <SettingRow
          rowId="appearance-theme"
          title="Theme"
          description="Switch between dark and light"
          first
        >
          <div className="flex items-center gap-2 text-ink-muted">
            {theme === "dark" ? (
              <Moon className="h-3.5 w-3.5" aria-hidden />
            ) : (
              <Sun className="h-3.5 w-3.5" aria-hidden />
            )}
            <Switch
              checked={theme === "light"}
              onCheckedChange={() => toggleTheme()}
              aria-label={
                theme === "dark" ? "Switch to light theme" : "Switch to dark theme"
              }
              title={
                theme === "dark" ? "Switch to light theme" : "Switch to dark theme"
              }
            />
          </div>
        </SettingRow>
      </SettingsCard>

      <SettingsCard label="Accent">
        <SettingRow
          rowId="appearance-accent"
          title="Accent color"
          description="Default is neutral (high-contrast). Choose a hue or any custom shade."
          first
          stacked
        >
          <AccentColorPicker />
        </SettingRow>
      </SettingsCard>

      <SettingsCard label="Color themes">
        <SettingRow
          rowId="appearance-theme-library"
          title="Custom themes"
          description="Named token overrides layered on top of the factory dark/light palette. Import JSON or create in the editor."
          first
          stacked
        >
          <ThemeLibrary />
        </SettingRow>
      </SettingsCard>
    </div>
  )
}

const COMPACTION_MODE_OPTIONS = [
  { value: "standard", label: "Standard" },
  { value: "turn_pair", label: "Turn pair" },
]

/** Shared plugin-prefs save helper for the Behavior section. Round-trips all
 * fields so a partial patch doesn't accidentally wipe siblings. */
const useSavePlugins = () => {
  const { config, save } = useProviderConfig()
  return async (patch: Partial<PluginPrefs>) => {
    if (!config) return
    const plugins = config.plugins
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
        learningRequireHumanApproval: plugins.learningRequireHumanApproval ?? false,
        learningRequireVerifiedMemory: plugins.learningRequireVerifiedMemory ?? false,
        artifacts: plugins.artifacts ?? true,
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
        costMode: plugins.costMode ?? "auto",
        costModelsLow: plugins.costModelsLow ?? [],
        costModelsMedium: plugins.costModelsMedium ?? [],
        costModelsHigh: plugins.costModelsHigh ?? [],
        ...patch,
      },
    })
  }
}

/** Behavior section — session defaults (isolation) and secret storage live
 * inside `ProviderSettingsForm` today, coupled to the same form state as
 * Connections/Models (one save flow, one set of mutations). Splitting that
 * component so "Behavior" fields render standalone here is exactly the
 * restructuring the build brief defers to a later wave ("mount the EXISTING
 * ProviderSettingsForm content for now — restructuring it is a SEPARATE
 * later wave"); for this pass, Behavior cross-links to Models & Connections
 * instead of duplicating a live form in two places. */
const BehaviorContent = () => {
  const setSettingsSection = useAppStore((s) => s.setSettingsSection)
  const defaultPermissionMode = useAppStore((s) => s.defaultPermissionMode)
  const setDefaultPermissionMode = useAppStore((s) => s.setDefaultPermissionMode)
  const { config, isLoading } = useProviderConfig()
  const savePlugins = useSavePlugins()

  const plugins = config?.plugins

  return (
    <div className="flex flex-col gap-3">
      <SettingsCard>
        <SettingRow
          rowId="behavior-permissions"
          title="Permissions"
          description="Bypass applies in Agent mode; AskUserQuestion still appears. Plan, Ask, and Flex keep their own safeguards."
          first
        >
          <Select
            items={PERMISSION_MODE_OPTIONS}
            value={defaultPermissionMode}
            onValueChange={(v) => {
              if (v == null) return
              setDefaultPermissionMode(v as PermissionMode)
            }}
          >
            <SelectTrigger
              id="default-permission-mode"
              aria-label="Default permission mode"
              className="w-full"
              size="sm"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {PERMISSION_MODE_OPTIONS.map((opt) => (
                  <SelectItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </SettingRow>
        <SettingRow
          rowId="behavior-isolation"
          title="Default isolation"
          description="New sessions can opt into a git worktree sandbox — configured together with your provider connection."
        >
          <Button
            variant="link"
            size="xs"
            onClick={() => setSettingsSection("models")}
            className="h-auto p-0 text-accent"
          >
            Open Models & Connections
          </Button>
        </SettingRow>
        <SettingRow
          rowId="behavior-secret-storage"
          title="Secret storage"
          description="Where the encryption key for your stored API keys lives — configured together with your provider connection."
        >
          <Button
            variant="link"
            size="xs"
            onClick={() => setSettingsSection("models")}
            className="h-auto p-0 text-accent"
          >
            Open Models & Connections
          </Button>
        </SettingRow>
      </SettingsCard>

      {/* Auto mode */}
      <SettingsCard label="Auto mode">
        <SettingRow
          rowId="behavior-auto-mode"
          title="Auto routing"
          description="Show an &ldquo;Auto&rdquo; option in the model picker. Auto turns use the router model below and inject delegation rules."
          first
        >
          <Switch
            checked={plugins?.autoMode ?? false}
            onCheckedChange={(on) => void savePlugins({ autoMode: on })}
            disabled={isLoading || !plugins}
            aria-label="Auto routing"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-auto-router-model"
          title="Router model"
          description="Provider/model id used for Auto turns (e.g. anthropic/claude-sonnet-4-5). Empty = session default."
        >
          <Input
            value={plugins?.autoModeRouterModel ?? ""}
            onChange={(e) =>
              void savePlugins({ autoModeRouterModel: e.target.value || undefined })
            }
            placeholder="anthropic/claude-sonnet-4-5"
            disabled={isLoading || !plugins}
            className="w-64"
            aria-label="Auto router model"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-delegation-rules"
          title="Delegation rules"
          description="Injected when Auto mode is on and the project has no delegation.md. Empty = built-in defaults."
          stacked
        >
          <Textarea
            value={plugins?.delegationRules ?? ""}
            onChange={(e) => void savePlugins({ delegationRules: e.target.value })}
            placeholder="Leave blank to use built-in defaults. Project delegation.md overrides this."
            disabled={isLoading || !plugins}
            rows={4}
            className="font-mono text-xs"
            aria-label="Delegation rules"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-mode-switch-veto-ms"
          title="Mode switch veto window (ms)"
          description="How long the UI shows a countdown before auto-accepting a SwitchMode proposal."
        >
          <Input
            type="number"
            min={500}
            max={30000}
            step={500}
            value={plugins?.modeSwitchVetoMs ?? 2000}
            onChange={(e) =>
              void savePlugins({
                modeSwitchVetoMs: Math.max(500, parseInt(e.target.value, 10) || 2000),
              })
            }
            disabled={isLoading || !plugins}
            className="w-28"
            aria-label="Mode switch veto window in milliseconds"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-cost-mode"
          title="Cost mode"
          description="Auto always starts low-cost with low reasoning, then may escalate via SetRouting. Fixed modes restrict which models and effort levels the SetRouting tool may pick."
        >
          <select
            value={plugins?.costMode ?? "auto"}
            onChange={(e) => void savePlugins({ costMode: e.target.value })}
            disabled={isLoading || !plugins}
            className="h-9 rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
            aria-label="Cost mode"
          >
            <option value="auto">Auto (escalate freely)</option>
            <option value="low">Low only</option>
            <option value="medium">Low + Medium</option>
            <option value="high">High only</option>
          </select>
        </SettingRow>
        <SettingRow
          rowId="behavior-cost-models-low"
          title="Low-cost models"
          description="Comma-separated provider/model ids for the low cost tier (fast, cheap). Auto starts here."
          stacked
        >
          <Input
            value={(plugins?.costModelsLow ?? []).join(", ")}
            onChange={(e) =>
              void savePlugins({
                costModelsLow: e.target.value
                  .split(",")
                  .map((m) => m.trim())
                  .filter(Boolean),
              })
            }
            placeholder="anthropic/claude-haiku-4-5, openai/gpt-4.1-mini"
            disabled={isLoading || !plugins}
            className="font-mono text-xs w-full"
            aria-label="Low cost tier models"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-cost-models-medium"
          title="Medium-cost models"
          description="Comma-separated provider/model ids for the medium cost tier (balanced)."
          stacked
        >
          <Input
            value={(plugins?.costModelsMedium ?? []).join(", ")}
            onChange={(e) =>
              void savePlugins({
                costModelsMedium: e.target.value
                  .split(",")
                  .map((m) => m.trim())
                  .filter(Boolean),
              })
            }
            placeholder="anthropic/claude-sonnet-4-5, openai/gpt-4.1"
            disabled={isLoading || !plugins}
            className="font-mono text-xs w-full"
            aria-label="Medium cost tier models"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-cost-models-high"
          title="High-cost models"
          description="Comma-separated provider/model ids for the high cost tier (powerful, expensive)."
          stacked
        >
          <Input
            value={(plugins?.costModelsHigh ?? []).join(", ")}
            onChange={(e) =>
              void savePlugins({
                costModelsHigh: e.target.value
                  .split(",")
                  .map((m) => m.trim())
                  .filter(Boolean),
              })
            }
            placeholder="anthropic/claude-opus-4-5, openai/o3"
            disabled={isLoading || !plugins}
            className="font-mono text-xs w-full"
            aria-label="High cost tier models"
          />
        </SettingRow>
      </SettingsCard>

      {/* Compaction */}
      <SettingsCard label="Context compaction">
        <SettingRow
          rowId="behavior-auto-compact"
          title="Auto compact"
          description="Proactively compact context when usage nears the threshold (reactive compaction on overflow always applies)."
          first
        >
          <Switch
            checked={plugins?.autoCompact ?? true}
            onCheckedChange={(on) => void savePlugins({ autoCompact: on })}
            disabled={isLoading || !plugins}
            aria-label="Auto compact"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-compact-threshold"
          title="Threshold (%)"
          description="Percentage of the context window at which proactive compaction fires (1–100)."
        >
          <Input
            type="number"
            min={50}
            max={99}
            step={5}
            value={plugins?.autoCompactThresholdPercent ?? 85}
            onChange={(e) =>
              void savePlugins({
                autoCompactThresholdPercent: Math.min(
                  99,
                  Math.max(50, parseInt(e.target.value, 10) || 85),
                ),
              })
            }
            disabled={isLoading || !plugins}
            className="w-24"
            aria-label="Compaction threshold percent"
          />
        </SettingRow>
        <SettingRow
          rowId="behavior-compaction-mode"
          title="Compaction strategy"
          description="Standard summarizes the oldest messages; Turn pair compresses user↔assistant pairs into nicknames."
        >
          <Select
            value={plugins?.compactionMode ?? "standard"}
            onValueChange={(v) => {
              if (!v) return
              void savePlugins({ compactionMode: v })
            }}
          >
            <SelectTrigger
              id="compaction-mode"
              aria-label="Compaction strategy"
              className="w-36"
              size="sm"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {COMPACTION_MODE_OPTIONS.map((opt) => (
                  <SelectItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </SettingRow>
      </SettingsCard>
    </div>
  )
}

/** Settings shell composition root — assembles all sections (see DESIGN.md
 * Settings) and mounts the persistent-nav `SettingsShell`.
 * This one component now backs all four legacy routes (settings / customize
 * / automations / memory); `App.tsx` renders it for each and `appStore`
 * preselects the matching nav section (see `setRoute`). */
type SettingsPageProps = {
  embedded?: boolean
}

export const SettingsPage = ({ embedded = false }: SettingsPageProps) => {
  return (
    <SettingsShell
      embedded={embedded}
      titleFor={(section) => TITLES[section]}
      descriptionFor={(section) => DESCRIPTIONS[section]}
      sections={{
        general: <GeneralContent />,
        appearance: <AppearanceContent />,
        models: <ProviderSettingsForm />,
        behavior: <BehaviorContent />,
        "remote-access": <RemoteAccessContent />,
        memory: <MemoryContent />,
        indexing: <IndexingContent />,
        "tools-mcp": <CustomizeContent />,
        ...(AUTOMATIONS_UI_ENABLED ? { automations: <AutomationsContent /> } : {}),
        diagnostics: <DiagnosticsContent />,
      }}
    />
  )
}
