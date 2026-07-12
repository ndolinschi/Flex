import { useEffect, useState } from "react"
import {
  ConfirmDialog,
  ProviderConnectionForm,
  ProviderProfileList,
  SecretStorageSection,
} from "../molecules"
import { useProviderProfiles } from "../../hooks/useProviderProfiles"
import { useProviderConfig } from "../../hooks/useProviderConfig"
import { useModels } from "../../hooks/useModels"
import { platformType } from "../../lib/tauri"
import type {
  ProviderProfileInput,
  ProviderProfileView,
  SecretStorageMode,
} from "../../lib/types"
import { sessionHasActivity, useAppStore } from "../../stores/appStore"

/** `"provider/model, other/model"` <-> ordered id array, matching the wire
 * shape `ProviderProfile::fallback_models` actually stores (a comma-joined
 * string — see `src-tauri/src/config.rs`). The UI only ever deals in
 * ordered arrays; this is the one place the string is touched. */
const parseFallbacks = (raw?: string): string[] =>
  raw
    ? raw
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
    : []

const serializeFallbacks = (ids: string[]): string | undefined =>
  ids.length > 0 ? ids.join(", ") : undefined

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
  const [fallbackModels, setFallbackModels] = useState<string[]>([])
  const [defaultIsolation, setDefaultIsolation] = useState("never")
  const [formError, setFormError] = useState<string | null>(null)
  const [validateMessage, setValidateMessage] = useState<string | null>(null)
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null)

  // Platform gate for the Security section's "System Keychain" option —
  // detected once via `@tauri-apps/plugin-os` (mock mode reports "macos" so
  // preview always shows it). Backend already rejects keychain on non-mac;
  // this just keeps the option from being offered where it can't work.
  const [platform, setPlatform] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    void platformType().then((p) => {
      if (!cancelled) setPlatform(p)
    })
    return () => {
      cancelled = true
    }
  }, [])
  const isMac = platform === "macos"

  const hydrateFromProfile = (p: ProviderProfileView) => {
    setEditingId(p.id)
    setLabel(p.label)
    setProvider(p.provider)
    setBaseUrl(p.baseUrl ?? "")
    setRegion(p.region ?? "")
    setDefaultModel(p.defaultModel ?? "")
    setFallbackModels(parseFallbacks(p.fallbackModels))
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
    setFallbackModels([])
    setDefaultIsolation("never")
    setFormError(null)
    setValidateMessage(null)
  }

  const editingProfile = profiles.find((p) => p.id === editingId)
  const selectedBuiltin = builtinProviders.find((p) => p.id === provider)
  const requiresKey = selectedBuiltin?.requiresApiKey ?? true
  const hasStoredKey = editingProfile?.hasKey ?? false
  const isBedrock = provider === "bedrock"

  // Default model is scoped to THIS connection's provider (picking a model
  // from a different provider than the connection makes no sense); fallbacks
  // may span any provider (that's the point of a failover chain).
  const defaultModelOptions = provider
    ? models.filter((m) => m.providerId === provider)
    : models

  const buildInput = (): ProviderProfileInput => ({
    id: editingId ?? undefined,
    label: label.trim(),
    provider,
    apiKey: apiKey.trim() || undefined,
    baseUrl: baseUrl.trim() || undefined,
    region: region.trim() || undefined,
    defaultModel: defaultModel.trim() || undefined,
    fallbackModels: serializeFallbacks(fallbackModels),
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
    const state = useAppStore.getState()
    const activeSessionId = state.activeSessionId
    if (!activeSessionId) return
    // Gate on prior activity — same predicate Composer.tsx's
    // `handleModelChange` uses — so a fresh session with no turns yet
    // doesn't get a "Provider changed" row before the user has said
    // anything.
    if (!sessionHasActivity(state, activeSessionId)) return
    const nextLabel = profiles.find((p) => p.id === id)?.label ?? fallbackLabel ?? id
    state.addSessionLogRow(activeSessionId, `Provider changed to ${nextLabel}`)
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

  return (
    // Self-managed gap (matches AutomationsContent's pattern) — each section
    // below cancels its own `mb-8` so the parent SettingsShell's `gap-3` is
    // the only spacing applied between them, instead of stacking both.
    <div className="flex flex-col gap-3">
      <ProviderProfileList
        profiles={profiles}
        editingId={editingId}
        isActivating={isActivating}
        onNewConnection={clearToCreateMode}
        onSelect={hydrateFromProfile}
        onActivate={(id) => void handleActivate(id)}
        onDelete={setPendingDeleteId}
      />

      <ProviderConnectionForm
        editingId={editingId}
        editingLabel={editingProfile?.label}
        label={label}
        provider={provider}
        apiKey={apiKey}
        baseUrl={baseUrl}
        region={region}
        defaultModel={defaultModel}
        fallbackModels={fallbackModels}
        defaultIsolation={defaultIsolation}
        hasStoredKey={hasStoredKey}
        isBedrock={isBedrock}
        models={models}
        defaultModelOptions={defaultModelOptions}
        builtinProviders={builtinProviders}
        modelsLoading={modelsLoading}
        formError={formError}
        validateMessage={validateMessage}
        isValidating={isValidating}
        isSaving={isUpserting || isActivating}
        onLabelChange={setLabel}
        onProviderChange={setProvider}
        onApiKeyChange={setApiKey}
        onBaseUrlChange={setBaseUrl}
        onRegionChange={setRegion}
        onDefaultModelChange={setDefaultModel}
        onFallbackModelsChange={setFallbackModels}
        onDefaultIsolationChange={setDefaultIsolation}
        onValidate={() => void handleValidate()}
        onSave={() => void handleSave()}
      />

      <SecretStorageSection
        secretStorage={config?.secretStorage}
        isMac={isMac}
        disabled={isSettingSecretStorage || !config}
        error={secretStorageFormError ?? secretStorageError ?? null}
        onChange={(mode) => void handleSecretStorageChange(mode)}
      />

      <ConfirmDialog
        open={pendingDeleteId !== null}
        title="Delete connection?"
        description="This removes the stored connection and its API key. This can't be undone."
        confirmLabel="Delete"
        danger
        onConfirm={() => void handleDelete()}
        onCancel={() => setPendingDeleteId(null)}
      />
    </div>
  )
}
