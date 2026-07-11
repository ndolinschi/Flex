import { useState } from "react"
import { Badge, Button, TextInput } from "../atoms"
import {
  ConfirmDialog,
  ErrorBanner,
  FieldRow,
  ModelSelect,
  SettingsSection,
} from "../molecules"
import { useProviderProfiles } from "../../hooks/useProviderProfiles"
import { useProviderConfig } from "../../hooks/useProviderConfig"
import { useModels } from "../../hooks/useModels"
import type {
  ProviderProfileInput,
  ProviderProfileView,
  SecretStorageMode,
} from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

export const ProviderSettingsForm = () => {
  const {
    profiles,
    isLoading: profilesLoading,
    upsert,
    remove,
    activate,
    validate,
    isUpserting,
    isActivating,
    isValidating,
  } = useProviderProfiles()
  const { models, builtinProviders, isLoading: modelsLoading } = useModels()
  const setRoute = useAppStore((s) => s.setRoute)
  const {
    config,
    setSecretStorage,
    isSettingSecretStorage,
    secretStorageError,
  } = useProviderConfig()
  const [secretStorageFormError, setSecretStorageFormError] = useState<
    string | null
  >(null)

  const handleSecretStorageChange = async (mode: SecretStorageMode) => {
    setSecretStorageFormError(null)
    try {
      await setSecretStorage(mode)
    } catch (err) {
      setSecretStorageFormError(err instanceof Error ? err.message : String(err))
    }
  }

  // `editingId` is `null` in "create" mode (form cleared, "New connection"),
  // or a profile id in "edit" mode (row clicked, form hydrated from it).
  const [editingId, setEditingId] = useState<string | null>(null)
  const [label, setLabel] = useState("")
  const [provider, setProvider] = useState("")
  const [baseUrl, setBaseUrl] = useState("")
  const [region, setRegion] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [defaultModel, setDefaultModel] = useState("")
  const [fallbackModels, setFallbackModels] = useState("")
  const [defaultIsolation, setDefaultIsolation] = useState("never")
  const [formError, setFormError] = useState<string | null>(null)
  const [validateMessage, setValidateMessage] = useState<string | null>(null)
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null)

  const hydrateFromProfile = (p: ProviderProfileView) => {
    setEditingId(p.id)
    setLabel(p.label)
    setProvider(p.provider)
    setBaseUrl(p.baseUrl ?? "")
    setRegion(p.region ?? "")
    setDefaultModel(p.defaultModel ?? "")
    setFallbackModels(p.fallbackModels ?? "")
    setDefaultIsolation((p.defaultIsolation as string | undefined) ?? "never")
    setApiKey("")
    setFormError(null)
    setValidateMessage(null)
  }

  const clearToCreateMode = () => {
    setEditingId(null)
    setLabel("")
    setProvider("")
    setBaseUrl("")
    setRegion("")
    setApiKey("")
    setDefaultModel("")
    setFallbackModels("")
    setDefaultIsolation("never")
    setFormError(null)
    setValidateMessage(null)
  }

  const editingProfile = profiles.find((p) => p.id === editingId)
  const selectedBuiltin = builtinProviders.find((p) => p.id === provider)
  const requiresKey = selectedBuiltin?.requiresApiKey ?? true
  const hasStoredKey = editingProfile?.hasKey ?? false
  const isBedrock = provider === "bedrock"

  const buildInput = (): ProviderProfileInput => ({
    id: editingId ?? undefined,
    label: label.trim(),
    provider,
    apiKey: apiKey.trim() || undefined,
    baseUrl: baseUrl.trim() || undefined,
    region: region.trim() || undefined,
    defaultModel: defaultModel.trim() || undefined,
    fallbackModels: fallbackModels.trim() || undefined,
    defaultIsolation,
  })

  const validateForm = (): string | null => {
    if (!label.trim()) return "Connection name is required"
    if (!provider.trim()) return "Select a provider"
    if (requiresKey && !apiKey.trim() && !hasStoredKey) {
      return "API key is required for this provider"
    }
    return null
  }

  const handleValidate = async () => {
    setFormError(null)
    setValidateMessage(null)
    const err = validateForm()
    if (err) {
      setFormError(err)
      return
    }
    try {
      // Validates the exact values currently in the form (including a
      // freshly pasted key) — never env/stored config. See
      // `commands::validate_profile`.
      const found = await validate(buildInput())
      setValidateMessage(`Validated — ${found.length} model(s) available`)
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleSave = async () => {
    setFormError(null)
    setValidateMessage(null)
    const err = validateForm()
    if (err) {
      setFormError(err)
      return
    }
    try {
      const wasActive = editingProfile?.isActive ?? false
      const saved = await upsert(buildInput())
      // Newly created connections (or the only one) become the active one
      // automatically on the backend; explicitly (re-)activate on every save
      // so editing the *currently* active connection's key/region takes
      // effect immediately without a separate "Activate" click.
      await activate(saved.id)
      if (!wasActive) logProviderChange(saved.id, saved.label)
      setApiKey("")
      setRoute("chat")
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
    }
  }

  const logProviderChange = (id: string, fallbackLabel?: string) => {
    const activeSessionId = useAppStore.getState().activeSessionId
    if (!activeSessionId) return
    const label = profiles.find((p) => p.id === id)?.label ?? fallbackLabel ?? id
    useAppStore.getState().addSessionLogRow(activeSessionId, `Provider changed to ${label}`)
  }

  const handleActivate = async (id: string) => {
    setFormError(null)
    try {
      const wasActive = profiles.find((p) => p.id === id)?.isActive ?? false
      await activate(id)
      if (!wasActive) logProviderChange(id)
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleDelete = async () => {
    if (!pendingDeleteId) return
    setFormError(null)
    try {
      await remove(pendingDeleteId)
      if (editingId === pendingDeleteId) clearToCreateMode()
      setPendingDeleteId(null)
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
      setPendingDeleteId(null)
    }
  }

  if (profilesLoading) {
    return (
      <div className="flex items-center justify-center py-8 text-sm text-ink-muted">
        Loading configuration…
      </div>
    )
  }

  const selectClassName =
    "h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]"

  return (
    <>
      <SettingsSection
        title="Connections"
        description="Named provider connections you can switch between (e.g. two AWS accounts)"
        actions={
          <Button size="sm" variant="secondary" onClick={clearToCreateMode}>
            New connection
          </Button>
        }
      >
        {profiles.length === 0 ? (
          <div className="px-4 py-3 text-sm text-ink-muted">
            No connections yet — fill out the form below and save to create one.
          </div>
        ) : (
          profiles.map((p) => (
            <div
              key={p.id}
              role="button"
              tabIndex={0}
              onClick={() => hydrateFromProfile(p)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") hydrateFromProfile(p)
              }}
              className={cn(
                "flex cursor-pointer items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-surface-muted",
                editingId === p.id && "bg-surface-muted",
              )}
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-medium text-ink">
                    {p.label}
                  </span>
                  <Badge variant="muted">{p.provider}</Badge>
                  {p.isActive ? <Badge variant="success">Active</Badge> : null}
                  {!p.hasKey && p.provider !== "ollama" ? (
                    <Badge variant="warning">No key</Badge>
                  ) : null}
                </div>
                {p.region || p.baseUrl ? (
                  <p className="mt-0.5 truncate text-xs text-ink-faint">
                    {p.region ?? p.baseUrl}
                  </p>
                ) : null}
              </div>
              <div
                className="flex shrink-0 items-center gap-1.5"
                onClick={(e) => e.stopPropagation()}
              >
                {!p.isActive ? (
                  <Button
                    size="sm"
                    variant="secondary"
                    isLoading={isActivating}
                    onClick={() => void handleActivate(p.id)}
                  >
                    Activate
                  </Button>
                ) : null}
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setPendingDeleteId(p.id)}
                >
                  Delete
                </Button>
              </div>
            </div>
          ))
        )}
      </SettingsSection>

      <form
        onSubmit={(e) => {
          e.preventDefault()
          void handleSave()
        }}
      >
        <SettingsSection
          title={editingId ? "Edit connection" : "New connection"}
          description="Native provider for the agent loop"
        >
          <FieldRow label="Name" htmlFor="label" hint="e.g. &quot;AWS work&quot;">
            <TextInput
              id="label"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="AWS work"
            />
          </FieldRow>

          <FieldRow label="Provider" htmlFor="provider">
            <select
              id="provider"
              value={provider}
              onChange={(e) => setProvider(e.target.value)}
              className={selectClassName}
            >
              <option value="">Select provider</option>
              {builtinProviders.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </FieldRow>

          <FieldRow
            label="API key"
            htmlFor="apiKey"
            hint={
              isBedrock
                ? hasStoredKey && !apiKey
                  ? "A Bedrock API key is already stored — leave blank to keep it"
                  : "Paste your Bedrock API key (bearer token) — sent as Authorization: Bearer <token>. Stored encrypted locally, never in browser storage; see Security below for the storage backend."
                : hasStoredKey && !apiKey
                  ? "A key is already stored — leave blank to keep it"
                  : "Stored encrypted locally, never in browser storage; see Security below for the storage backend"
            }
          >
            <TextInput
              id="apiKey"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              autoComplete="off"
              placeholder={hasStoredKey ? "••••••••" : isBedrock ? "Bedrock API key" : "sk-…"}
            />
          </FieldRow>

          {isBedrock ? (
            <FieldRow
              label="Region"
              htmlFor="region"
              hint="AWS region for Bedrock, e.g. us-east-1 or eu-west-1 (defaults to us-east-1)"
            >
              <TextInput
                id="region"
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                placeholder="us-east-1"
              />
            </FieldRow>
          ) : (
            <FieldRow
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
            </FieldRow>
          )}
        </SettingsSection>

        <SettingsSection title="Models">
          <FieldRow
            label="Default model"
            htmlFor="defaultModel"
            hint={
              models.length > 0
                ? undefined
                : "Optional — provider/model id (e.g. anthropic/claude-sonnet-4-5)"
            }
          >
            {models.length > 0 ? (
              <ModelSelect
                id="defaultModel"
                label=""
                models={models}
                value={defaultModel}
                onChange={setDefaultModel}
                isLoading={modelsLoading}
                placeholder="Select default model"
                className="gap-0"
              />
            ) : (
              <TextInput
                id="defaultModel"
                value={defaultModel}
                onChange={(e) => setDefaultModel(e.target.value)}
                placeholder="provider/model-id"
              />
            )}
          </FieldRow>

          <FieldRow
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
          </FieldRow>
        </SettingsSection>

        {/* Plugins moved to the Customize page; `plugins` state is kept hydrated
            from config so buildInput() round-trips the current values on save. */}

        <SettingsSection title="Behavior">
          <FieldRow
            label="Default isolation"
            htmlFor="defaultIsolation"
            hint="New sessions can opt into a git worktree sandbox"
          >
            <select
              id="defaultIsolation"
              value={defaultIsolation}
              onChange={(e) => setDefaultIsolation(e.target.value)}
              className={selectClassName}
            >
              <option value="never">Never</option>
              <option value="optional">Optional (when git allows)</option>
              <option value="required">Required</option>
            </select>
          </FieldRow>
        </SettingsSection>

        <div className="flex items-center justify-end gap-3">
          {formError ? (
            <ErrorBanner message={formError} className="mr-auto" />
          ) : validateMessage ? (
            <p className="mr-auto text-sm text-success" role="status">
              {validateMessage}
            </p>
          ) : null}

          <Button
            type="button"
            variant="ghost"
            isLoading={isValidating}
            onClick={() => void handleValidate()}
          >
            Validate
          </Button>
          <Button type="submit" isLoading={isUpserting || isActivating}>
            Save & continue
          </Button>
        </div>
      </form>

      <SettingsSection
        title="Security"
        description="Where the encryption key for your stored API keys lives"
      >
        <FieldRow
          label="Secret storage"
          htmlFor="secretStorage"
          hint={
            config?.secretStorage === "keychain"
              ? "System Keychain is OS-protected, but macOS may prompt for access — especially on dev builds that re-sign on every rebuild."
              : "Local file stores the encryption key on disk, readable by your user account — no system prompts, ever. System Keychain is OS-protected but may prompt."
          }
        >
          <select
            id="secretStorage"
            value={config?.secretStorage ?? "file"}
            disabled={isSettingSecretStorage || !config}
            onChange={(e) =>
              void handleSecretStorageChange(e.target.value as SecretStorageMode)
            }
            className={selectClassName}
          >
            <option value="file">Local file (no system prompts)</option>
            <option value="keychain">System Keychain (OS-protected)</option>
          </select>
        </FieldRow>
        {secretStorageFormError || secretStorageError ? (
          <div className="px-4 py-3">
            <ErrorBanner
              message={secretStorageFormError ?? secretStorageError ?? ""}
            />
          </div>
        ) : null}
      </SettingsSection>

      <ConfirmDialog
        open={pendingDeleteId !== null}
        title="Delete connection?"
        description="This removes the stored connection and its API key. This can't be undone."
        confirmLabel="Delete"
        danger
        onConfirm={() => void handleDelete()}
        onCancel={() => setPendingDeleteId(null)}
      />
    </>
  )
}
