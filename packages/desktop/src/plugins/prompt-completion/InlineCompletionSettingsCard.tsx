import { useState } from "react"
import { Sparkles } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner } from "../../components/atoms"
import { Switch } from "@/components/ui/switch"
import {
  ErrorBanner,
  SettingRow,
  SettingsCard,
} from "../../components/molecules"
import { useInlineCompletionPrefs } from "../../hooks/useInlineCompletionPrefs"
import { INLINE_COMPLETION_ENABLED } from "../../lib/featureFlags"
import { CompletionSetupModal } from "./CompletionSetupModal"

export const InlineCompletionSettingsCard = () => {
  const { prefs, isLoading, save, isSaving, error } = useInlineCompletionPrefs()
  const [setupOpen, setSetupOpen] = useState(false)
  const [localError, setLocalError] = useState<string | null>(null)

  if (!INLINE_COMPLETION_ENABLED) return null

  const configured = !!(prefs?.providerId && prefs?.modelId)
  const modelLabel = configured
    ? `${prefs!.providerId}/${prefs!.modelId}`
    : "Not configured"

  const toggleEnabled = async (checked: boolean) => {
    setLocalError(null)
    if (checked && !configured) {
      setSetupOpen(true)
      return
    }
    if (!prefs) return
    try {
      await save({ ...prefs, enabled: checked })
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : String(err))
    }
  }

  return (
    <>
      <SettingsCard
        label="Prompt completions"
        description="Ghost-text suggestions in the composer and Prompt tab."
      >
        {(error || localError) && (
          <div className="px-3.5 pt-3">
            <ErrorBanner
              message={error ?? localError ?? ""}
              onDismiss={() => setLocalError(null)}
            />
          </div>
        )}
        <SettingRow
          first
          rowId="inline-completion-enabled"
          title="Inline suggestions"
          description="Tab accepts a suggestion when @ / trays are closed."
        >
          {isLoading ? (
            <Spinner className="h-3.5 w-3.5" />
          ) : (
            <Switch
              checked={!!prefs?.enabled && configured}
              onCheckedChange={(v) => void toggleEnabled(v)}
              aria-label="Enable inline prompt completions"
              title="Enable inline prompt completions"
              disabled={isSaving}
            />
          )}
        </SettingRow>
        <SettingRow
          rowId="inline-completion-model"
          title="Completion model"
          description={modelLabel}
        >
          <Button
            variant="ghost"
            size="sm"
            className="h-6 gap-1 px-1.5 text-xs"
            onClick={() => setSetupOpen(true)}
          >
            <Sparkles className="h-3 w-3" aria-hidden />
            {configured ? "Change…" : "Set up…"}
          </Button>
        </SettingRow>
      </SettingsCard>

      <CompletionSetupModal
        open={setupOpen}
        onClose={() => setSetupOpen(false)}
        onDismiss={() => {
          if (!prefs) return
          void save({ ...prefs, setupDismissed: true })
        }}
      />
    </>
  )
}
