import { useEffect, useState } from "react"
import {
  ChatgptSignInDialog,
  ConfirmDialog,
  CopilotSignInDialog,
  ProviderConnectionForm,
  ProviderProfileList,
  SecretStorageSection,
} from "../molecules"
import { Spinner } from "../atoms"
import { useChatgptAuth } from "../../hooks/useChatgptAuth"
import { useCopilotAuth } from "../../hooks/useCopilotAuth"
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

  const [screen, setScreen] = useState<"list" | "editor">("list")
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
  const [copilotSignInOpen, setCopilotSignInOpen] = useState(false)
  const [chatgptSignInOpen, setChatgptSignInOpen] = useState(false)
  const pushToast = useAppStore((s) => s.pushToast)

  const isCopilotProvider = provider === "copilot"
  const isChatgptProvider = provider === "chatgpt"
  const {
    signedIn: copilotSignedIn,
    start: copilotStart,
    wait: copilotWait,
    cancel: copilotCancel,
    refetchStatus: refetchCopilotStatus,
  } = useCopilotAuth(isCopilotProvider)
  const {
    signedIn: chatgptSignedIn,
    start: chatgptStart,
    wait: chatgptWait,
    cancel: chatgptCancel,
    refetchStatus: refetchChatgptStatus,
  } = useChatgptAuth(isChatgptProvider)

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
    setScreen("editor")
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
    setScreen("editor")
  }

  const returnToList = () => {
    setScreen("list")
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
    if (provider === "copilot") {
      if (!apiKey.trim() && !hasStoredKey && !copilotSignedIn) {
        return "Sign in with GitHub or paste a Copilot token"
      }
      return null
    }
    if (provider === "chatgpt") {
      if (!chatgptSignedIn) {
        return "Sign in with ChatGPT Plus/Pro"
      }
      return null
    }
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
      if (editingId === pendingDeleteId) returnToList()
      setPendingDeleteId(null)
    } catch (err) {
      setFormError(err instanceof Error ? err.message : String(err))
      setPendingDeleteId(null)
    }
  }

  if (profilesLoading) {
    return (
      <div className="flex items-center justify-center gap-2 py-8 text-sm text-ink-muted">
        <Spinner size="sm" />
        Loading configuration…
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      {screen === "list" ? (
        <>
          <ProviderProfileList
            profiles={profiles}
            editingId={null}
            isActivating={isActivating}
            onNewConnection={clearToCreateMode}
            onSelect={hydrateFromProfile}
            onActivate={(id) => void handleActivate(id)}
            onDelete={setPendingDeleteId}
          />

          <SecretStorageSection
            secretStorage={config?.secretStorage}
            isMac={isMac}
            disabled={isSettingSecretStorage || !config}
            error={secretStorageFormError ?? secretStorageError ?? null}
            onChange={(mode) => void handleSecretStorageChange(mode)}
          />
        </>
      ) : (
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
          copilotSignedIn={copilotSignedIn}
          chatgptSignedIn={chatgptSignedIn}
          models={models}
          defaultModelOptions={defaultModelOptions}
          builtinProviders={builtinProviders}
          modelsLoading={modelsLoading}
          formError={formError}
          validateMessage={validateMessage}
          isValidating={isValidating}
          isSaving={isUpserting || isActivating}
          onLabelChange={setLabel}
          onProviderChange={(value) => {
            setProvider(value)
            if (!label.trim() && value) {
              const preset = builtinProviders.find((p) => p.id === value)?.label
              if (preset) setLabel(preset)
            }
          }}
          onApiKeyChange={setApiKey}
          onBaseUrlChange={setBaseUrl}
          onRegionChange={setRegion}
          onDefaultModelChange={setDefaultModel}
          onFallbackModelsChange={setFallbackModels}
          onDefaultIsolationChange={setDefaultIsolation}
          onValidate={() => void handleValidate()}
          onSave={() => void handleSave()}
          onCancel={returnToList}
          onCopilotSignIn={() => setCopilotSignInOpen(true)}
          onChatgptSignIn={() => setChatgptSignInOpen(true)}
        />
      )}

      <ConfirmDialog
        open={pendingDeleteId !== null}
        title="Delete connection?"
        description="This removes the stored connection and its API key. This can't be undone."
        confirmLabel="Delete"
        danger
        onConfirm={() => void handleDelete()}
        onCancel={() => setPendingDeleteId(null)}
      />

      <CopilotSignInDialog
        open={copilotSignInOpen}
        onClose={() => setCopilotSignInOpen(false)}
        onSuccess={() => {
          setCopilotSignInOpen(false)
          void refetchCopilotStatus()
          pushToast("Signed in to GitHub Copilot", "success")
        }}
        start={copilotStart}
        wait={copilotWait}
        cancel={copilotCancel}
      />

      <ChatgptSignInDialog
        open={chatgptSignInOpen}
        onClose={() => setChatgptSignInOpen(false)}
        onSuccess={() => {
          setChatgptSignInOpen(false)
          void refetchChatgptStatus()
          pushToast("Signed in to ChatGPT", "success")
        }}
        start={chatgptStart}
        wait={chatgptWait}
        cancel={chatgptCancel}
      />
    </div>
  )
}
