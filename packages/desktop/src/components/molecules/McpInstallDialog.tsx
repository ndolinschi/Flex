import { useEffect, useState } from "react"
import { Button, TextInput } from "../atoms"
import { ErrorBanner } from "./ErrorBanner"
import { FieldRow } from "./SettingsSection"
import type { McpCatalogEntry } from "../../lib/mcpCatalog"
import { cn } from "../../lib/utils"

type McpInstallDialogProps = {
  entry: McpCatalogEntry | null
  isLoading: boolean
  error: string | null
  onCancel: () => void
  onInstall: (values: { args: Record<string, string>; env: Record<string, string> }) => void
}

/** Small inline-form dialog collecting the args/env values a catalog entry
 * needs before install (e.g. filesystem's path, GitHub's token). Modeled
 * after `ConfirmDialog`'s modal shell but with its own field body, since
 * `ConfirmDialog`'s `children` slot doesn't carry per-field validation. */
export const McpInstallDialog = ({
  entry,
  isLoading,
  error,
  onCancel,
  onInstall,
}: McpInstallDialogProps) => {
  const [argValues, setArgValues] = useState<Record<string, string>>({})
  const [envValues, setEnvValues] = useState<Record<string, string>>({})
  const [validationError, setValidationError] = useState<string | null>(null)

  useEffect(() => {
    if (!entry) return
    setArgValues(Object.fromEntries(entry.argKeys.map((a) => [a.key, ""])))
    setEnvValues(Object.fromEntries(entry.envKeys.map((e) => [e.name, ""])))
    setValidationError(null)
  }, [entry])

  useEffect(() => {
    if (!entry) return
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onCancel()
      }
    }
    document.addEventListener("keydown", handleKey)
    return () => document.removeEventListener("keydown", handleKey)
  }, [entry, onCancel])

  if (!entry) return null

  const handleInstall = () => {
    setValidationError(null)
    for (const arg of entry.argKeys) {
      if (arg.required && !argValues[arg.key]?.trim()) {
        setValidationError(`${arg.label} is required`)
        return
      }
    }
    for (const env of entry.envKeys) {
      if (env.required && !envValues[env.name]?.trim()) {
        setValidationError(`${env.label} is required`)
        return
      }
    }
    onInstall({ args: argValues, env: envValues })
  }

  return (
    <div
      className="fixed inset-0 z-[100] flex items-start justify-center bg-black/20 pt-[100px] animate-backdrop-in"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="mcp-install-dialog-title"
        className={cn(
          "w-full max-w-[500px] rounded-xl border border-stroke-2 bg-panel p-4 shadow-lg",
          "animate-modal-in",
        )}
      >
        <h2 id="mcp-install-dialog-title" className="text-base font-semibold text-ink">
          Install {entry.name}
        </h2>
        <p className="mt-1 text-sm text-ink-muted">{entry.description}</p>

        <div className="mt-3 flex flex-col divide-y divide-stroke-3 rounded-lg border border-stroke-3">
          {entry.argKeys.map((arg) => (
            <FieldRow key={arg.key} label={arg.label + (arg.required ? " *" : "")} htmlFor={`mcp-install-arg-${arg.key}`}>
              <TextInput
                id={`mcp-install-arg-${arg.key}`}
                value={argValues[arg.key] ?? ""}
                onChange={(e) =>
                  setArgValues((prev) => ({ ...prev, [arg.key]: e.target.value }))
                }
                placeholder={arg.placeholder}
              />
            </FieldRow>
          ))}
          {entry.envKeys.map((env) => (
            <FieldRow key={env.name} label={env.label + (env.required ? " *" : "")} htmlFor={`mcp-install-env-${env.name}`}>
              <TextInput
                id={`mcp-install-env-${env.name}`}
                type={env.secret ? "password" : "text"}
                autoComplete="off"
                value={envValues[env.name] ?? ""}
                onChange={(e) =>
                  setEnvValues((prev) => ({ ...prev, [env.name]: e.target.value }))
                }
                placeholder={env.placeholder}
              />
            </FieldRow>
          ))}
        </div>

        {error || validationError ? (
          <div className="mt-3">
            <ErrorBanner
              message={error ?? validationError ?? ""}
              onDismiss={() => setValidationError(null)}
            />
          </div>
        ) : null}

        <div className="mt-4 flex justify-end gap-1.5">
          <Button size="sm" variant="secondary" disabled={isLoading} onClick={onCancel}>
            Cancel
          </Button>
          <Button size="sm" variant="primary" isLoading={isLoading} onClick={handleInstall}>
            Install
          </Button>
        </div>
      </div>
    </div>
  )
}
