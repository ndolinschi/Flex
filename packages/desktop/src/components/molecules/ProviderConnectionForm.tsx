import { useState } from "react"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import { TextInput } from "../atoms"
import { ErrorBanner } from "./ErrorBanner"
import { FieldRow, SettingsSection } from "./SettingsSection"
import { ModelMultiSelect } from "./ModelMultiSelect"
import { ModelSelect } from "./ModelSelect"
import { ProviderPicker } from "./ProviderPicker"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"

const ISOLATION_ITEMS = [
  { value: "never", label: "Never" },
  { value: "optional", label: "Optional (when git allows)" },
  { value: "required", label: "Required" },
]

type ProviderConnectionFormProps = {
  editingId: string | null
  editingLabel?: string
  label: string
  provider: string
  apiKey: string
  baseUrl: string
  region: string
  defaultModel: string
  fallbackModels: string[]
  defaultIsolation: string
  hasStoredKey: boolean
  isBedrock: boolean
  /** GitHub Copilot: device-flow / editor sign-in is present. */
  copilotSignedIn?: boolean
  /** ChatGPT Plus/Pro: OAuth tokens are discoverable. */
  chatgptSignedIn?: boolean
  models: ModelInfoDto[]
  defaultModelOptions: ModelInfoDto[]
  builtinProviders: BuiltinProvider[]
  modelsLoading: boolean
  formError: string | null
  validateMessage: string | null
  isValidating: boolean
  isSaving: boolean
  onLabelChange: (value: string) => void
  onProviderChange: (value: string) => void
  onApiKeyChange: (value: string) => void
  onBaseUrlChange: (value: string) => void
  onRegionChange: (value: string) => void
  onDefaultModelChange: (value: string) => void
  onFallbackModelsChange: (value: string[]) => void
  onDefaultIsolationChange: (value: string) => void
  onValidate: () => void
  onSave: () => void
  /** Return to the connections list without saving. */
  onCancel?: () => void
  onCopilotSignIn?: () => void
  onChatgptSignIn?: () => void
}

/** Connection create/edit form (fields + models + isolation + save footer). */
export const ProviderConnectionForm = ({
  editingId,
  editingLabel,
  label,
  provider,
  apiKey,
  baseUrl,
  region,
  defaultModel,
  fallbackModels,
  defaultIsolation,
  hasStoredKey,
  isBedrock,
  copilotSignedIn = false,
  chatgptSignedIn = false,
  models,
  defaultModelOptions,
  builtinProviders,
  modelsLoading,
  formError,
  validateMessage,
  isValidating,
  isSaving,
  onLabelChange,
  onProviderChange,
  onApiKeyChange,
  onBaseUrlChange,
  onRegionChange,
  onDefaultModelChange,
  onFallbackModelsChange,
  onDefaultIsolationChange,
  onValidate,
  onSave,
  onCancel,
  onCopilotSignIn,
  onChatgptSignIn,
}: ProviderConnectionFormProps) => {
  const isCopilot = provider === "copilot"
  const isChatgpt = provider === "chatgpt"
  const [showTokenPaste, setShowTokenPaste] = useState(false)

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault()
        onSave()
      }}
    >
      <SettingsSection
        title={editingId ? `Edit connection — ${editingLabel ?? label}` : "New connection"}
        description="Native provider for the agent loop"
        className="mb-0"
      >
        <FieldRow
          label="Name"
          htmlFor="label"
          hint='Required — a label for this connection, e.g. "GitHub Copilot" or "AWS work"'
        >
          <TextInput
            id="label"
            value={label}
            onChange={(e) => onLabelChange(e.target.value)}
            placeholder="AWS work"
          />
        </FieldRow>

        <FieldRow
          label="Provider"
          htmlFor="provider"
          // Full-width tile grid — two-column FieldRow leaves uneven side
          // padding around DeepSeek / Copilot / … chips.
          className="@[640px]/settings:grid-cols-1 @[640px]/settings:gap-2"
        >
          <ProviderPicker
            providers={builtinProviders}
            value={provider}
            onChange={onProviderChange}
          />
        </FieldRow>

        {isCopilot ? (
          <>
            <FieldRow
              label="GitHub Copilot"
              htmlFor="copilotSignIn"
              hint={
                copilotSignedIn || hasStoredKey
                  ? "Signed in — validate to list models, then save"
                  : "Sign in with GitHub device flow, or paste an existing OAuth token"
              }
            >
              <div className="flex flex-col gap-2">
                <p className="text-sm text-ink">
                  {copilotSignedIn || hasStoredKey ? (
                    <span className="text-success">Signed in</span>
                  ) : (
                    <span className="text-ink-muted">Not signed in</span>
                  )}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    id="copilotSignIn"
                    size="sm"
                    onClick={onCopilotSignIn}
                  >
                    {copilotSignedIn || hasStoredKey
                      ? "Sign in again"
                      : "Sign in with GitHub"}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowTokenPaste((v) => !v)}
                  >
                    {showTokenPaste ? "Hide token field" : "Use existing token"}
                  </Button>
                </div>
              </div>
            </FieldRow>
            {showTokenPaste ? (
              <FieldRow
                label="GitHub token"
                htmlFor="apiKey"
                hint={
                  hasStoredKey && !apiKey
                    ? "A token is already stored — leave blank to keep it"
                    : "Paste a gho_/ghu_ token with Copilot access. Stored encrypted locally."
                }
              >
                <TextInput
                  id="apiKey"
                  type="password"
                  value={apiKey}
                  onChange={(e) => onApiKeyChange(e.target.value)}
                  autoComplete="off"
                  placeholder={hasStoredKey ? "••••••••" : "gho_…"}
                />
              </FieldRow>
            ) : null}
          </>
        ) : isChatgpt ? (
          <FieldRow
            label="ChatGPT subscription"
            htmlFor="chatgptSignIn"
            hint={
              chatgptSignedIn
                ? "Signed in — validate to list models, then save"
                : "Sign in with your ChatGPT Plus/Pro account (Codex subscription)"
            }
          >
            <div className="flex flex-col gap-2">
              <p className="text-sm text-ink">
                {chatgptSignedIn ? (
                  <span className="text-success">Signed in</span>
                ) : (
                  <span className="text-ink-muted">Not signed in</span>
                )}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  id="chatgptSignIn"
                  size="sm"
                  onClick={onChatgptSignIn}
                >
                  {chatgptSignedIn ? "Sign in again" : "Sign in with ChatGPT"}
                </Button>
              </div>
            </div>
          </FieldRow>
        ) : (
          <>
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
                onChange={(e) => onApiKeyChange(e.target.value)}
                autoComplete="off"
                placeholder={
                  hasStoredKey ? "••••••••" : isBedrock ? "Bedrock API key" : "sk-…"
                }
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
                  onChange={(e) => onRegionChange(e.target.value)}
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
                  onChange={(e) => onBaseUrlChange(e.target.value)}
                  placeholder="https://api.example.com/v1"
                />
              </FieldRow>
            )}
          </>
        )}
      </SettingsSection>

      <SettingsSection
        title="Models for this connection"
        description="This connection's default model and failover chain — used whenever it's active"
        rowId="models-defaults"
        className="mb-0"
      >
        <FieldRow
          label="Default model"
          htmlFor="defaultModel"
          hint={
            provider
              ? defaultModelOptions.length > 0
                ? undefined
                : "No models available for this provider yet — validate the connection first"
              : "Select a provider above first"
          }
        >
          <ModelSelect
            id="defaultModel"
            label=""
            models={defaultModelOptions}
            value={defaultModel}
            onChange={onDefaultModelChange}
            isLoading={modelsLoading}
            disabled={!provider}
            placeholder="Select default model"
            builtinProviders={builtinProviders}
            className="gap-0"
          />
        </FieldRow>

        <FieldRow
          label="Fallback models"
          htmlFor="fallbackModels"
          hint="Ordered failover chain — tried in order when the default fails. Can span any provider."
        >
          <ModelMultiSelect
            id="fallbackModels"
            label=""
            models={models}
            value={fallbackModels}
            onChange={onFallbackModelsChange}
            isLoading={modelsLoading}
            builtinProviders={builtinProviders}
          />
        </FieldRow>
      </SettingsSection>

      {/* Plugins moved to the Customize page; `plugins` state is kept hydrated
          from config so buildInput() round-trips the current values on save. */}

      <SettingsSection title="Behavior" rowId="behavior-isolation" className="mb-0">
        <FieldRow
          label="Default isolation for new sessions"
          htmlFor="defaultIsolation"
          hint="Sessions can override this when created — this only sets the starting default"
        >
          <Select
            items={ISOLATION_ITEMS}
            value={defaultIsolation}
            onValueChange={(v) => {
              if (v == null) return
              onDefaultIsolationChange(v)
            }}
          >
            <SelectTrigger id="defaultIsolation" className="w-full" size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {ISOLATION_ITEMS.map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
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

        {onCancel ? (
          <Button type="button" variant="ghost" onClick={onCancel}>
            Back
          </Button>
        ) : null}
        <Button
          type="button"
          variant="ghost"
          disabled={isValidating}
          onClick={onValidate}
        >
          {isValidating ? <Spinner data-icon="inline-start" /> : null}
          Validate
        </Button>
        <Button type="submit" disabled={isSaving}>
          {isSaving ? <Spinner data-icon="inline-start" /> : null}
          Save & continue
        </Button>
      </div>
    </form>
  )
}
