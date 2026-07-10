import { useEffect, useState } from "react"
import { Button, TextInput } from "../atoms"
import { ErrorBanner, FormField, ModelSelect } from "../molecules"
import { useProviderConfig } from "../../hooks/useProviderConfig"
import { useModels } from "../../hooks/useModels"
import type { PluginPrefs, SaveProviderConfigInput } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"

const DEFAULT_PLUGINS: PluginPrefs = {
  search: true,
  learning: false,
  verifier: false,
}

export const ProviderSettingsForm = () => {
  const { config, isLoading, save, validate, isSaving, isValidating, saveError } =
    useProviderConfig()
  const { models, builtinProviders, isLoading: modelsLoading } = useModels()
  const setRoute = useAppStore((s) => s.setRoute)

  const [provider, setProvider] = useState("")
  const [baseUrl, setBaseUrl] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [defaultModel, setDefaultModel] = useState("")
  const [plugins, setPlugins] = useState<PluginPrefs>(DEFAULT_PLUGINS)
  const [fallbackModels, setFallbackModels] = useState("")
  const [defaultIsolation, setDefaultIsolation] = useState("never")
  const [formError, setFormError] = useState<string | null>(null)
  const [validateMessage, setValidateMessage] = useState<string | null>(null)

  useEffect(() => {
    if (!config) return
    setProvider(config.preferredProvider ?? "")
    setBaseUrl(config.baseUrl ?? "")
    setDefaultModel(config.defaultModel ?? "")
    setPlugins(config.plugins ?? DEFAULT_PLUGINS)
    setFallbackModels((config.fallbackModels ?? []).join(", "))
    setDefaultIsolation(
      (config.defaultIsolation as string | undefined) ?? "never",
    )
    setApiKey("")
  }, [config])

  const selectedBuiltin = builtinProviders.find((p) => p.id === provider)
  const requiresKey = selectedBuiltin?.requiresApiKey ?? true
  const hasStoredKey = config?.configuredProviders.includes(provider) ?? false

  const buildInput = (): SaveProviderConfigInput => ({
    preferredProvider: provider,
    apiKey: apiKey.trim() || undefined,
    baseUrl: baseUrl.trim() || undefined,
    defaultModel: defaultModel.trim() || undefined,
    plugins,
    fallbackModels: fallbackModels
      .split(/[,\n]/)
      .map((s) => s.trim())
      .filter(Boolean),
    defaultIsolation,
  })

  const handleValidate = async () => {
    setFormError(null)
    setValidateMessage(null)
    if (!provider.trim()) {
      setFormError("Select a provider")
      return
    }
    if (requiresKey && !apiKey.trim() && !hasStoredKey) {
      setFormError("API key is required for this provider")
      return
    }
    try {
      const found = await validate(buildInput())
      setValidateMessage(`Validated — ${found.length} model(s) available`)
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleSave = async () => {
    setFormError(null)
    setValidateMessage(null)
    if (!provider.trim()) {
      setFormError("Select a provider")
      return
    }
    if (requiresKey && !apiKey.trim() && !hasStoredKey) {
      setFormError("API key is required for this provider")
      return
    }
    try {
      await save(buildInput())
      setApiKey("")
      setRoute("chat")
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
    }
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-8 text-sm text-ink-muted">
        Loading configuration…
      </div>
    )
  }

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault()
        void handleSave()
      }}
    >
      <FormField label="Provider" htmlFor="provider">
        <select
          id="provider"
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
          className="h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]"
        >
          <option value="">Select provider</option>
          {builtinProviders.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </FormField>

      <FormField
        label="Base URL"
        htmlFor="baseUrl"
        hint="Optional host override (e.g. for Ollama or a proxy)"
      >
        <TextInput
          id="baseUrl"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.example.com/v1"
        />
      </FormField>

      <FormField
        label="API key"
        htmlFor="apiKey"
        hint={
          hasStoredKey && !apiKey
            ? "A key is already stored — leave blank to keep it"
            : "Stored securely in the OS keychain, never in browser storage"
        }
      >
        <TextInput
          id="apiKey"
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          autoComplete="off"
          placeholder={hasStoredKey ? "••••••••" : "sk-…"}
        />
      </FormField>

      {models.length > 0 ? (
        <ModelSelect
          id="defaultModel"
          label="Default model"
          models={models}
          value={defaultModel}
          onChange={setDefaultModel}
          isLoading={modelsLoading}
          placeholder="Select default model"
        />
      ) : (
        <FormField
          label="Default model"
          htmlFor="defaultModel"
          hint="Optional — provider/model id (e.g. anthropic/claude-sonnet-4-5)"
        >
          <TextInput
            id="defaultModel"
            value={defaultModel}
            onChange={(e) => setDefaultModel(e.target.value)}
            placeholder="provider/model-id"
          />
        </FormField>
      )}

      <FormField
        label="Fallback models"
        htmlFor="fallbackModels"
        hint="Comma-separated provider/model ids tried when the primary fails"
      >
        <TextInput
          id="fallbackModels"
          value={fallbackModels}
          onChange={(e) => setFallbackModels(e.target.value)}
          placeholder="openai/gpt-4.1, anthropic/claude-sonnet-4"
        />
      </FormField>

      {/* Plugins moved to the Customize page; `plugins` state is kept hydrated
          from config so buildInput() round-trips the current values on save. */}

      <FormField
        label="Default isolation"
        htmlFor="defaultIsolation"
        hint="New sessions can opt into a git worktree sandbox"
      >
        <select
          id="defaultIsolation"
          value={defaultIsolation}
          onChange={(e) => setDefaultIsolation(e.target.value)}
          className="h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]"
        >
          <option value="never">Never</option>
          <option value="optional">Optional (when git allows)</option>
          <option value="required">Required</option>
        </select>
      </FormField>

      {formError || saveError ? (
        <ErrorBanner message={formError ?? saveError ?? ""} />
      ) : null}

      {validateMessage ? (
        <p className="text-sm text-success" role="status">
          {validateMessage}
        </p>
      ) : null}

      <div className="flex gap-3">
        <Button
          type="button"
          variant="secondary"
          isLoading={isValidating}
          onClick={() => void handleValidate()}
        >
          Validate
        </Button>
        <Button type="submit" isLoading={isSaving}>
          Save & continue
        </Button>
      </div>
    </form>
  )
}
