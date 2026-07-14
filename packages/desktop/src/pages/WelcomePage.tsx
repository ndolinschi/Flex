import { useMemo, useState } from "react"
import { open as openDialog } from "@tauri-apps/plugin-dialog"
import { Button, Kbd, TextInput } from "../components/atoms"
import {
  CopilotSignInDialog,
  ErrorBanner,
  FormField,
  ModelSelect,
} from "../components/molecules"
import { useCopilotAuth } from "../hooks/useCopilotAuth"
import { useModels } from "../hooks/useModels"
import { useProviderProfiles } from "../hooks/useProviderProfiles"
import { useSessions } from "../hooks/useSessions"
import { isBrowserPreview, NATIVE_APP_REQUIRED } from "../lib/browserPreview"
import { newAgentCreateInput } from "../lib/sessions"
import type { ProviderProfileInput } from "../lib/types"
import { useAppStore } from "../stores/appStore"
import { cn } from "../lib/utils"

type Step = "provider" | "model" | "project"

const STEPS: Step[] = ["provider", "model", "project"]

const stepTitle: Record<Step, string> = {
  provider: "Add a provider",
  model: "Pick a default model",
  project: "Optional — open a project",
}

const stepHint: Record<Step, string> = {
  provider:
    "Choose a native provider. Paste an API key, or sign in with GitHub for Copilot. Credentials stay encrypted locally.",
  model: "This becomes the default for new agents. You can change it anytime in the composer.",
  project:
    "Open a folder to start in that workspace. Repo indexing runs automatically when the index plugin is enabled — skip to start chatting immediately.",
}

/** First-run wizard: provider key → model → optional project folder.
 * Goal: first turn in under ~2 minutes without the full Settings form. */
export const WelcomePage = () => {
  const { builtinProviders, models, isLoading: modelsLoading } = useModels()
  const { upsert, activate, isUpserting } = useProviderProfiles()
  const { createSession } = useSessions()
  const setSelectedModelId = useAppStore((s) => s.setSelectedModelId)
  const pushRecentCwd = useAppStore((s) => s.pushRecentCwd)
  const pushToast = useAppStore((s) => s.pushToast)

  const [step, setStep] = useState<Step>("provider")
  const [provider, setProvider] = useState("")
  const [apiKey, setApiKey] = useState("")
  const [modelId, setModelId] = useState("")
  const [projectPath, setProjectPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [copilotSignInOpen, setCopilotSignInOpen] = useState(false)
  const [showCopilotToken, setShowCopilotToken] = useState(false)

  const selectedBuiltin = builtinProviders.find((p) => p.id === provider)
  const requiresKey = selectedBuiltin?.requiresApiKey ?? true
  const isCopilot = provider === "copilot"
  const {
    signedIn: copilotSignedIn,
    start: copilotStart,
    wait: copilotWait,
    cancel: copilotCancel,
    refetchStatus: refetchCopilotStatus,
  } = useCopilotAuth(isCopilot)

  const providerModels = useMemo(
    () => (provider ? models.filter((m) => m.providerId === provider) : models),
    [models, provider],
  )

  const stepIndex = STEPS.indexOf(step)

  const handlePickProvider = (id: string) => {
    setProvider(id)
    setApiKey("")
    setError(null)
    setShowCopilotToken(false)
    const first = models.find((m) => m.providerId === id)
    if (first) setModelId(first.id)
  }

  const handleProviderNext = async () => {
    setError(null)
    if (!provider) {
      setError("Select a provider")
      return
    }
    if (isCopilot) {
      if (!apiKey.trim() && !copilotSignedIn) {
        setError("Sign in with GitHub or paste a Copilot token")
        return
      }
    } else if (requiresKey && !apiKey.trim()) {
      setError("API key is required for this provider")
      return
    }

    setBusy(true)
    try {
      const label = selectedBuiltin?.label ?? provider
      const input: ProviderProfileInput = {
        label,
        provider,
        apiKey: apiKey.trim() || undefined,
        defaultModel: modelId || undefined,
      }
      const saved = await upsert(input)
      await activate(saved.id)
      if (saved.defaultModel) setModelId(saved.defaultModel)
      else if (!modelId && providerModels[0]) setModelId(providerModels[0].id)
      setStep("model")
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const handleModelNext = () => {
    setError(null)
    if (!modelId.trim()) {
      setError("Select a model")
      return
    }
    setSelectedModelId(modelId)
    setStep("project")
  }

  const handlePickFolder = async () => {
    setError(null)
    try {
      if (isBrowserPreview()) {
        setError(NATIVE_APP_REQUIRED)
        return
      }
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "Open project folder",
      })
      if (typeof selected === "string" && selected.trim()) {
        setProjectPath(selected)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleFinish = async (withProject: boolean) => {
    setError(null)
    setBusy(true)
    try {
      if (modelId) setSelectedModelId(modelId)
      const cwd = withProject ? projectPath ?? undefined : undefined
      if (cwd) pushRecentCwd(cwd)
      await createSession(newAgentCreateInput(cwd, modelId || null, null))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setBusy(false)
    }
  }

  const selectClassName =
    "h-9 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]"

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="mx-auto flex w-full max-w-[var(--welcome-rail)] flex-1 flex-col justify-center px-4 py-8">
        <p className="mb-1.5 text-xs font-medium uppercase tracking-widest text-ink-faint">
          Agent Desktop
        </p>
        <h1 className="mb-2 text-xl font-semibold text-ink">{stepTitle[step]}</h1>
        <p className="mb-4 text-sm text-ink-muted">{stepHint[step]}</p>

        <ol className="mb-6 flex items-center gap-2" aria-label="Setup steps">
          {STEPS.map((s, i) => (
            <li key={s} className="flex items-center gap-2">
              <span
                className={cn(
                  "flex h-6 w-6 items-center justify-center rounded-full text-xs font-medium",
                  i < stepIndex
                    ? "bg-fill-2 text-ink ring-1 ring-stroke-2"
                    : i === stepIndex
                      ? "bg-accent text-accent-fg"
                      : "bg-fill-4 text-ink-faint",
                )}
                aria-current={i === stepIndex ? "step" : undefined}
              >
                {i + 1}
              </span>
              {i < STEPS.length - 1 ? (
                <span className="h-px w-6 bg-stroke-3" aria-hidden />
              ) : null}
            </li>
          ))}
        </ol>

        {error ? (
          <div className="mb-4">
            <ErrorBanner message={error} onDismiss={() => setError(null)} />
          </div>
        ) : null}

        {step === "provider" ? (
          <div className="flex max-w-md flex-col gap-3">
            <FormField label="Provider" htmlFor="welcome-provider">
              <select
                id="welcome-provider"
                className={selectClassName}
                value={provider}
                onChange={(e) => handlePickProvider(e.target.value)}
                aria-label="Provider"
                disabled={modelsLoading || busy}
              >
                <option value="">Select…</option>
                {builtinProviders.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
            </FormField>
            {isCopilot ? (
              <div className="flex flex-col gap-2 rounded-md border border-border bg-surface px-3 py-3">
                <p className="text-sm text-ink">
                  {copilotSignedIn ? (
                    <span className="text-success">Signed in to GitHub Copilot</span>
                  ) : (
                    <span className="text-ink-muted">Not signed in</span>
                  )}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    onClick={() => setCopilotSignInOpen(true)}
                    disabled={busy}
                  >
                    {copilotSignedIn ? "Sign in again" : "Sign in with GitHub"}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowCopilotToken((v) => !v)}
                    disabled={busy}
                  >
                    {showCopilotToken ? "Hide token field" : "Use existing token"}
                  </Button>
                </div>
                {showCopilotToken ? (
                  <FormField label="GitHub Copilot token" htmlFor="welcome-copilot-token">
                    <TextInput
                      id="welcome-copilot-token"
                      type="password"
                      autoComplete="off"
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder="gho_…"
                      aria-label="GitHub Copilot token"
                      disabled={busy}
                    />
                  </FormField>
                ) : null}
              </div>
            ) : requiresKey ? (
              <FormField label="API key" htmlFor="welcome-api-key">
                <TextInput
                  id="welcome-api-key"
                  type="password"
                  autoComplete="off"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-…"
                  aria-label="API key"
                  disabled={busy}
                />
              </FormField>
            ) : (
              <p className="text-xs text-ink-muted">
                This provider does not need an API key.
              </p>
            )}
            <div className="mt-2 flex justify-end">
              <Button
                onClick={() => void handleProviderNext()}
                isLoading={busy || isUpserting}
                disabled={!provider}
              >
                Continue
              </Button>
            </div>
          </div>
        ) : null}

        {step === "model" ? (
          <div className="flex max-w-md flex-col gap-3">
            <ModelSelect
              id="onboarding-model"
              models={providerModels}
              value={modelId}
              onChange={setModelId}
              isLoading={modelsLoading}
              disabled={busy}
              builtinProviders={builtinProviders}
            />
            <div className="mt-2 flex justify-between gap-2">
              <Button variant="ghost" onClick={() => setStep("provider")} disabled={busy}>
                Back
              </Button>
              <Button onClick={handleModelNext} disabled={!modelId || busy}>
                Continue
              </Button>
            </div>
          </div>
        ) : null}

        {step === "project" ? (
          <div className="flex max-w-md flex-col gap-3">
            <div className="rounded-md border border-border bg-surface px-3 py-3">
              <p className="text-sm text-ink">
                {projectPath ? (
                  <>
                    <span className="text-ink-muted">Folder: </span>
                    <span className="break-all font-medium">{projectPath}</span>
                  </>
                ) : (
                  <span className="text-ink-muted">
                    No folder selected — you can open one later.
                  </span>
                )}
              </p>
              <div className="mt-3">
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void handlePickFolder()}
                  disabled={busy}
                >
                  {projectPath ? "Change folder" : "Open folder"}
                </Button>
              </div>
            </div>
            <div className="mt-2 flex flex-wrap justify-between gap-2">
              <Button variant="ghost" onClick={() => setStep("model")} disabled={busy}>
                Back
              </Button>
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="secondary"
                  onClick={() => void handleFinish(false)}
                  isLoading={busy && !projectPath}
                  disabled={busy}
                >
                  Skip & start chatting
                </Button>
                <Button
                  onClick={() => void handleFinish(true)}
                  isLoading={busy && !!projectPath}
                  disabled={busy || !projectPath}
                >
                  Start in folder
                </Button>
              </div>
            </div>
          </div>
        ) : null}

        <div className="mt-8 flex flex-wrap gap-3 text-xs text-ink-faint">
          <span>
            <Kbd>Enter</Kbd> send
          </span>
          <span>
            <Kbd>⌘</Kbd> + <Kbd>N</Kbd> new agent
          </span>
          <span>
            <Kbd>⌘</Kbd> + <Kbd>K</Kbd> search
          </span>
        </div>
      </div>

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
    </div>
  )
}
