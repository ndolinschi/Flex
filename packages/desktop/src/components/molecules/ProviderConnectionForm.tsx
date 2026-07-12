import { Button, TextInput } from "../atoms"
import { ErrorBanner } from "./ErrorBanner"
import { FieldRow, SettingsSection } from "./SettingsSection"
import { ModelMultiSelect } from "./ModelMultiSelect"
import { ModelSelect } from "./ModelSelect"
import type { BuiltinProvider, ModelInfoDto } from "../../lib/types"

const selectClassName =
  "h-8 w-full rounded-md border border-border bg-surface px-2.5 text-sm text-ink focus:border-accent focus:outline-none focus:[box-shadow:0_0_0_1px_var(--color-accent)]"

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
}: ProviderConnectionFormProps) => {
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
        <FieldRow label="Name" htmlFor="label" hint="e.g. &quot;AWS work&quot;">
          <TextInput
            id="label"
            value={label}
            onChange={(e) => onLabelChange(e.target.value)}
            placeholder="AWS work"
          />
        </FieldRow>

        <FieldRow label="Provider" htmlFor="provider">
          <select
            id="provider"
            value={provider}
            onChange={(e) => onProviderChange(e.target.value)}
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
            onChange={(e) => onApiKeyChange(e.target.value)}
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
          <select
            id="defaultIsolation"
            value={defaultIsolation}
            onChange={(e) => onDefaultIsolationChange(e.target.value)}
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
          onClick={onValidate}
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
