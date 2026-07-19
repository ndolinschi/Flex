import { useEffect, useMemo, useState } from "react"
import { createPortal } from "react-dom"
import { AlertCircleIcon, CheckCircle2Icon } from "lucide-react"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button, Spinner } from "../../components/atoms"
import { ErrorBanner, ModelSelect } from "../../components/molecules"
import { Button as ShadcnButton } from "@/components/ui/button"
import { useInlineCompletionPrefs } from "../../hooks/useInlineCompletionPrefs"
import { useModels } from "../../hooks/useModels"
import {
  OLLAMA_PULL_COMMAND,
  RECOMMENDED_OLLAMA_MODEL,
  normalizeCompletionModelId,
  qualifiedCompletionModelId,
} from "../../lib/inlineCompletion"
import type { InlineCompletionPrefs } from "../../lib/types"
import { checkInlineCompletionConnection } from "../../lib/tauri"
import { cn } from "../../lib/utils"

type Path = "ollama" | "provider"

type CompletionSetupModalProps = {
  open: boolean
  onClose: () => void
  /** Soft dismiss (set setupDismissed) vs just close after save. */
  onDismiss?: () => void
}

/**
 * First-run / change-model modal: connect Ollama (with pull guidance) or pick
 * any model already listed from connected providers.
 */
export const CompletionSetupModal = ({
  open,
  onClose,
  onDismiss,
}: CompletionSetupModalProps) => {
  const { prefs, save, isSaving } = useInlineCompletionPrefs()
  const { models, isLoading: modelsLoading, refetchModels } = useModels(open)
  const [path, setPath] = useState<Path>("ollama")
  const [providerId, setProviderId] = useState("")
  const [modelId, setModelId] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [checking, setChecking] = useState(false)
  const [checkMessage, setCheckMessage] = useState<string | null>(null)
  const [checkOk, setCheckOk] = useState<boolean | null>(null)

  const ollamaModels = useMemo(
    () => models.filter((m) => m.providerId === "ollama"),
    [models],
  )
  const otherProviders = useMemo(() => {
    const ids = new Set(
      models.map((m) => m.providerId).filter((id) => id !== "ollama"),
    )
    return [...ids].sort()
  }, [models])

  const providerModels = useMemo(() => {
    if (!providerId) return []
    return models.filter((m) => m.providerId === providerId)
  }, [models, providerId])

  const hasRecommended = ollamaModels.some(
    (m) =>
      m.id === RECOMMENDED_OLLAMA_MODEL ||
      m.id.startsWith(`${RECOMMENDED_OLLAMA_MODEL}:`) ||
      m.id.startsWith("qwen2.5"),
  )
  const ollamaReachable = ollamaModels.length > 0

  useEffect(() => {
    if (!open) return
    setError(null)
    setCopied(false)
    setCheckMessage(null)
    setCheckOk(null)
    if (prefs?.providerId && prefs?.modelId) {
      setProviderId(prefs.providerId)
      // Prefer bare model ids in local state; ModelSelect gets a qualified
      // value via `qualifiedCompletionModelId` below.
      setModelId(normalizeCompletionModelId(prefs.providerId, prefs.modelId))
      setPath(prefs.providerId === "ollama" ? "ollama" : "provider")
    } else {
      setPath("ollama")
      setProviderId("ollama")
      setModelId(RECOMMENDED_OLLAMA_MODEL)
    }
  }, [open, prefs?.providerId, prefs?.modelId])

  if (!open) return null

  const handleCopyPull = async () => {
    try {
      await navigator.clipboard.writeText(OLLAMA_PULL_COMMAND)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  const resolvedProviderId = path === "ollama" ? "ollama" : providerId
  const resolvedModelId =
    path === "ollama"
      ? modelId || RECOMMENDED_OLLAMA_MODEL
      : modelId

  const handleCheckConnection = async () => {
    setError(null)
    setCheckMessage(null)
    setCheckOk(null)
    const pid = resolvedProviderId
    const mid = resolvedModelId
    if (!pid || !mid) {
      setCheckOk(false)
      setCheckMessage("Pick a provider and model.")
      return
    }
    setChecking(true)
    try {
      const result = await checkInlineCompletionConnection(
        pid,
        normalizeCompletionModelId(pid, mid),
      )
      setCheckOk(result.ok)
      setCheckMessage(
        result.sample
          ? `${result.message} Sample: “${result.sample.slice(0, 60)}${result.sample.length > 60 ? "…" : ""}”`
          : result.message,
      )
    } catch (err) {
      setCheckOk(false)
      setCheckMessage(err instanceof Error ? err.message : String(err))
    } finally {
      setChecking(false)
    }
  }

  const handleSave = async () => {
    setError(null)
    const pid = resolvedProviderId
    const mid = normalizeCompletionModelId(pid, resolvedModelId)
    if (!pid || !mid) {
      setError("Pick a provider and model.")
      return
    }
    const next: InlineCompletionPrefs = {
      enabled: true,
      providerId: pid,
      modelId: mid,
      setupDismissed: false,
    }
    try {
      await save(next)
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleDismiss = () => {
    onDismiss?.()
    onClose()
  }

  return createPortal(
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) handleDismiss()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="completion-setup-title"
        className="flex w-full max-w-md flex-col gap-3 rounded-[var(--radius-card)] border border-stroke-3 bg-panel p-4 shadow-lg"
      >
        <div className="flex flex-col gap-1">
          <h2
            id="completion-setup-title"
            className="text-base font-semibold text-ink"
          >
            Prompt completions
          </h2>
          <p className="text-sm text-ink-muted">
            Ghost-text suggestions while you write prompts. Use a small local
            Ollama model or any connected provider.
          </p>
        </div>

        <div className="flex gap-1 rounded-md bg-fill-4 p-0.5">
          <ShadcnButton
            variant="ghost"
            onClick={() => {
              setPath("ollama")
              setProviderId("ollama")
              setModelId(RECOMMENDED_OLLAMA_MODEL)
            }}
            className={cn(
              "h-auto flex-1 rounded-md px-2 py-1.5 text-sm font-normal",
              path === "ollama"
                ? "bg-fill-2 text-ink hover:bg-fill-2"
                : "text-ink-muted hover:bg-transparent hover:text-ink",
            )}
          >
            Ollama
          </ShadcnButton>
          <ShadcnButton
            variant="ghost"
            onClick={() => {
              setPath("provider")
              setProviderId(otherProviders[0] ?? "")
              setModelId("")
            }}
            className={cn(
              "h-auto flex-1 rounded-md px-2 py-1.5 text-sm font-normal",
              path === "provider"
                ? "bg-fill-2 text-ink hover:bg-fill-2"
                : "text-ink-muted hover:bg-transparent hover:text-ink",
            )}
          >
            Existing connection
          </ShadcnButton>
        </div>

        {error ? <ErrorBanner message={error} onDismiss={() => setError(null)} /> : null}
        {checkMessage ? (
          <Alert variant={checkOk ? "default" : "destructive"}>
            {checkOk ? <CheckCircle2Icon /> : <AlertCircleIcon />}
            <AlertDescription
              className={checkOk ? "text-foreground" : "text-destructive"}
            >
              {checkMessage}
            </AlertDescription>
          </Alert>
        ) : null}

        {path === "ollama" ? (
          <div className="flex flex-col gap-2 text-sm">
            {modelsLoading ? (
              <div className="flex items-center gap-2 text-ink-muted">
                <Spinner className="h-3.5 w-3.5" />
                Checking Ollama…
              </div>
            ) : (
              <>
                {!ollamaReachable ? (
                  <>
                    <p className="text-ink-secondary">
                      Ollama is not listed in the engine yet. If the daemon is
                      running and you have models pulled, click Refresh models
                      (or Check connection) — Flex registers local Ollama on
                      rebuild without needing env vars.
                    </p>
                    <a
                      href="https://ollama.com/download"
                      target="_blank"
                      rel="noreferrer"
                      className="text-accent hover:underline"
                    >
                      Download Ollama
                    </a>
                  </>
                ) : null}
                {!hasRecommended ? (
                  <div className="flex flex-col gap-1.5 rounded-md border border-stroke-3 bg-fill-4/50 px-2.5 py-2">
                    <p className="text-ink-secondary">
                      Recommended: pull a small model, then refresh or save:
                    </p>
                    <div className="flex items-center gap-2">
                      <code className="min-w-0 flex-1 truncate text-xs">
                        {OLLAMA_PULL_COMMAND}
                      </code>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => void handleCopyPull()}
                      >
                        {copied ? "Copied" : "Copy"}
                      </Button>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="self-start"
                      onClick={() => void refetchModels()}
                    >
                      Refresh models
                    </Button>
                  </div>
                ) : null}
                <label className="flex flex-col gap-1">
                  <span className="text-xs text-ink-muted">Model</span>
                  {ollamaModels.length > 0 ? (
                    <ModelSelect
                      id="completion-ollama-model"
                      label=""
                      models={ollamaModels}
                      value={qualifiedCompletionModelId("ollama", modelId)}
                      onChange={(id) =>
                        setModelId(normalizeCompletionModelId("ollama", id))
                      }
                      isLoading={modelsLoading}
                      placeholder={RECOMMENDED_OLLAMA_MODEL}
                    />
                  ) : (
                    <input
                      type="text"
                      className="h-8 rounded-md border border-stroke-3 bg-elevated px-2 text-sm text-ink outline-none focus:border-stroke-2"
                      value={normalizeCompletionModelId("ollama", modelId)}
                      onChange={(e) =>
                        setModelId(
                          normalizeCompletionModelId("ollama", e.target.value),
                        )
                      }
                      placeholder={RECOMMENDED_OLLAMA_MODEL}
                    />
                  )}
                </label>
              </>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-2 text-sm">
            {modelsLoading ? (
              <div className="flex items-center gap-2 text-ink-muted">
                <Spinner className="h-3.5 w-3.5" />
                Loading models…
              </div>
            ) : otherProviders.length === 0 ? (
              <p className="text-ink-secondary">
                No cloud providers are registered yet. Add a connection under
                Settings → Models, or use Ollama.
              </p>
            ) : (
              <>
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs text-ink-muted">Provider</span>
                  <select
                    className="h-9 w-full rounded-md border border-stroke-3 bg-elevated px-3 text-sm text-ink outline-none focus:border-stroke-2"
                    value={providerId}
                    onChange={(e) => {
                      setProviderId(e.target.value)
                      setModelId("")
                    }}
                  >
                    {otherProviders.map((id) => (
                      <option key={id} value={id}>
                        {id}
                      </option>
                    ))}
                  </select>
                </label>
                <ModelSelect
                  id="completion-provider-model"
                  label="Model"
                  models={providerModels}
                  value={
                    providerId
                      ? qualifiedCompletionModelId(providerId, modelId)
                      : modelId
                  }
                  onChange={(id) =>
                    setModelId(
                      providerId
                        ? normalizeCompletionModelId(providerId, id)
                        : id,
                    )
                  }
                  isLoading={modelsLoading}
                  placeholder="Select a model…"
                />
              </>
            )}
          </div>
        )}

        <div className="flex justify-between gap-2 pt-1">
          <Button
            variant="ghost"
            size="sm"
            disabled={
              checking ||
              isSaving ||
              (path === "ollama" && !resolvedModelId.trim()) ||
              (path === "provider" && (!providerId || !modelId))
            }
            onClick={() => void handleCheckConnection()}
          >
            {checking ? "Checking…" : "Check connection"}
          </Button>
          <div className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={handleDismiss}>
              Not now
            </Button>
            <Button
              size="sm"
              disabled={
                isSaving ||
                (path === "ollama" && !resolvedModelId.trim()) ||
                (path === "provider" && (!providerId || !modelId))
              }
              onClick={() => void handleSave()}
            >
              {isSaving ? "Saving…" : "Save"}
            </Button>
          </div>
        </div>
      </div>
    </div>,
    document.body,
  )
}
