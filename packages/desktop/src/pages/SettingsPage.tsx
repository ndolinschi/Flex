import { Moon, Sun } from "lucide-react"
import { SettingsShell } from "../components/templates"
import { SettingsCard, SettingRow, SETTINGS_NAV_ITEMS, AccentColorPicker } from "../components/molecules"
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
import { ProviderSettingsForm } from "../components/organisms"
import { AUTOMATIONS_UI_ENABLED } from "../lib/featureFlags"
import { AutomationsContent } from "./settings/AutomationsSection"
import { CustomizeContent } from "./settings/CustomizeSection"
import { DiagnosticsContent } from "./settings/DiagnosticsSection"
import { IndexingContent } from "./settings/IndexingSection"
import { MemoryContent } from "./settings/MemorySection"
import { RemoteAccessContent } from "./settings/RemoteAccessSection"
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
