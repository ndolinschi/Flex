import { Moon, Sun } from "@/components/icons"
import { SettingsShell } from "../components/templates"
import { SettingsCard, SettingRow, SETTINGS_NAV_ITEMS, AccentColorPicker } from "../components/molecules"
import { Toggle } from "../components/atoms"
import { ProviderSettingsForm } from "../components/organisms"
import { AUTOMATIONS_UI_ENABLED } from "../lib/featureFlags"
import { AutomationsContent } from "./settings/AutomationsSection"
import { CustomizeContent } from "./settings/CustomizeSection"
import { DiagnosticsContent } from "./settings/DiagnosticsSection"
import { IndexingContent } from "./settings/IndexingSection"
import { MemoryContent } from "./settings/MemorySection"
import type { SettingsSectionId } from "../lib/settingsSearchIndex"
import type { PermissionMode } from "../lib/types"
import { useAppStore } from "../stores/appStore"

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
        <Toggle
          checked={notificationsEnabled}
          onChange={setNotificationsEnabled}
          label="Toggle system notifications"
        />
      </SettingRow>
      <SettingRow
        rowId="general-completion-sound"
        title="Completion sound"
        description="Play a short chime whenever a turn finishes, including the session you're viewing."
      >
        <Toggle
          checked={completionSoundEnabled}
          onChange={setCompletionSoundEnabled}
          label="Toggle completion sound"
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
            <Toggle
              checked={theme === "light"}
              onChange={() => toggleTheme()}
              label={
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
    </div>
  )
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

  return (
    <SettingsCard>
      <SettingRow
        rowId="behavior-permissions"
        title="Permissions"
        description="Bypass applies in Agent mode; AskUserQuestion still appears. Plan, Ask, and Flex keep their own safeguards."
        first
      >
        <select
          value={defaultPermissionMode}
          onChange={(e) =>
            setDefaultPermissionMode(e.target.value as PermissionMode)
          }
          aria-label="Default permission mode"
          className="h-8 rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-stroke-2 focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-stroke-2)]"
        >
          {PERMISSION_MODE_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </SettingRow>
      <SettingRow
        rowId="behavior-isolation"
        title="Default isolation"
        description="New sessions can opt into a git worktree sandbox — configured together with your provider connection."
      >
        <button
          type="button"
          onClick={() => setSettingsSection("models")}
          className="text-xs text-accent hover:underline"
        >
          Open Models & Connections
        </button>
      </SettingRow>
      <SettingRow
        rowId="behavior-secret-storage"
        title="Secret storage"
        description="Where the encryption key for your stored API keys lives — configured together with your provider connection."
      >
        <button
          type="button"
          onClick={() => setSettingsSection("models")}
          className="text-xs text-accent hover:underline"
        >
          Open Models & Connections
        </button>
      </SettingRow>
    </SettingsCard>
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
        memory: <MemoryContent />,
        indexing: <IndexingContent />,
        "tools-mcp": <CustomizeContent />,
        ...(AUTOMATIONS_UI_ENABLED ? { automations: <AutomationsContent /> } : {}),
        diagnostics: <DiagnosticsContent />,
      }}
    />
  )
}
