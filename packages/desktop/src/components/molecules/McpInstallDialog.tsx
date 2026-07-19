import { useEffect, useState } from "react"
import { TextInput } from "../atoms"
import { ErrorBanner } from "./ErrorBanner"
import { FieldRow } from "./SettingsSection"
import type { McpCatalogEntry } from "../../lib/mcpCatalog"
import type { CatalogInstallValues } from "../../lib/mcp"
import { Spinner } from "@/components/ui/spinner"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

type McpInstallDialogProps = {
  entry: McpCatalogEntry | null
  /** When set, dialog is in configure mode (keep-blank-to-keep for secrets). */
  mode?: "install" | "configure"
  /** Prefill for non-secret fields when configuring an existing server. */
  initialValues?: CatalogInstallValues | null
  /** Secret env keys already stored (configure mode). */
  configuredSecretEnv?: string[]
  hasSecretArgs?: boolean
  isLoading: boolean
  error: string | null
  onCancel: () => void
  onInstall: (values: CatalogInstallValues) => void
}

/** Small inline-form dialog collecting the args/env values a catalog entry
 * needs before install (e.g. filesystem's path, Slack's bot token). Also
 * reused as "Configure" for already-installed catalog servers — secret
 * fields accept empty to keep the existing encrypted value. */
export const McpInstallDialog = ({
  entry,
  mode = "install",
  initialValues = null,
  configuredSecretEnv = [],
  hasSecretArgs = false,
  isLoading,
  error,
  onCancel,
  onInstall,
}: McpInstallDialogProps) => {
  const [argValues, setArgValues] = useState<Record<string, string>>({})
  const [envValues, setEnvValues] = useState<Record<string, string>>({})
  const [validationError, setValidationError] = useState<string | null>(null)

  const isConfigure = mode === "configure"

  useEffect(() => {
    if (!entry) return
    if (initialValues) {
      setArgValues({ ...initialValues.args })
      setEnvValues({ ...initialValues.env })
    } else {
      setArgValues(Object.fromEntries(entry.argKeys.map((a) => [a.key, ""])))
      setEnvValues(Object.fromEntries(entry.envKeys.map((e) => [e.name, ""])))
    }
    setValidationError(null)
  }, [entry, initialValues])

  const handleInstall = () => {
    if (!entry) return
    setValidationError(null)
    const configuredSecretSet = new Set(configuredSecretEnv)
    for (const arg of entry.argKeys) {
      const value = argValues[arg.key]?.trim() ?? ""
      if (!arg.required) continue
      if (value) continue
      if (isConfigure && arg.secret && hasSecretArgs) continue
      setValidationError(`${arg.label} is required`)
      return
    }
    for (const env of entry.envKeys) {
      const value = envValues[env.name]?.trim() ?? ""
      if (!env.required) continue
      if (value) continue
      if (isConfigure && env.secret && configuredSecretSet.has(env.name)) continue
      setValidationError(`${env.label} is required`)
      return
    }
    onInstall({ args: argValues, env: envValues })
  }

  const configuredSecretSet = new Set(configuredSecretEnv)

  return (
    <AlertDialog
      open={!!entry}
      onOpenChange={(next) => {
        if (!next) onCancel()
      }}
    >
      <AlertDialogContent
        size="sm"
        className="max-w-[min(100%,32rem)] sm:max-w-lg"
      >
        {entry ? (
          <>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {isConfigure ? `Configure ${entry.name}` : `Install ${entry.name}`}
              </AlertDialogTitle>
              <AlertDialogDescription>{entry.description}</AlertDialogDescription>
            </AlertDialogHeader>

            {entry.setupHint ? (
              <p className="text-base leading-snug text-ink-secondary">{entry.setupHint}</p>
            ) : null}
            {entry.docsUrl ? (
              <a
                href={entry.docsUrl}
                target="_blank"
                rel="noreferrer"
                className="inline-block text-sm text-accent hover:underline"
              >
                Docs
              </a>
            ) : null}

            <div className="flex flex-col divide-y divide-stroke-3 rounded-lg border border-stroke-3">
              {entry.argKeys.map((arg) => {
                const keepHint =
                  isConfigure && arg.secret && hasSecretArgs
                    ? "Leave blank to keep the stored value."
                    : arg.hint
                return (
                  <FieldRow
                    key={arg.key}
                    label={arg.label + (arg.required ? " *" : "")}
                    htmlFor={`mcp-install-arg-${arg.key}`}
                    hint={keepHint}
                  >
                    <TextInput
                      id={`mcp-install-arg-${arg.key}`}
                      type={arg.secret ? "password" : "text"}
                      autoComplete="off"
                      value={argValues[arg.key] ?? ""}
                      onChange={(e) =>
                        setArgValues((prev) => ({ ...prev, [arg.key]: e.target.value }))
                      }
                      placeholder={
                        isConfigure && arg.secret && hasSecretArgs
                          ? "••••••••"
                          : arg.placeholder
                      }
                    />
                  </FieldRow>
                )
              })}
              {entry.envKeys.map((env) => {
                const hasStored = configuredSecretSet.has(env.name)
                const keepHint =
                  isConfigure && env.secret && hasStored
                    ? "Leave blank to keep the stored value."
                    : env.hint
                return (
                  <FieldRow
                    key={env.name}
                    label={env.label + (env.required ? " *" : "")}
                    htmlFor={`mcp-install-env-${env.name}`}
                    hint={keepHint}
                  >
                    <TextInput
                      id={`mcp-install-env-${env.name}`}
                      type={env.secret ? "password" : "text"}
                      autoComplete="off"
                      value={envValues[env.name] ?? ""}
                      onChange={(e) =>
                        setEnvValues((prev) => ({ ...prev, [env.name]: e.target.value }))
                      }
                      placeholder={
                        isConfigure && env.secret && hasStored ? "••••••••" : env.placeholder
                      }
                    />
                  </FieldRow>
                )
              })}
            </div>

            {error || validationError ? (
              <ErrorBanner
                message={error ?? validationError ?? ""}
                onDismiss={() => setValidationError(null)}
              />
            ) : null}

            <AlertDialogFooter>
              <AlertDialogCancel disabled={isLoading}>Cancel</AlertDialogCancel>
              <AlertDialogAction
                disabled={isLoading}
                onClick={(e) => {
                  e.preventDefault()
                  if (isLoading) return
                  handleInstall()
                }}
              >
                {isLoading ? <Spinner data-icon="inline-start" /> : null}
                {isConfigure ? "Save" : "Install"}
              </AlertDialogAction>
            </AlertDialogFooter>
          </>
        ) : null}
      </AlertDialogContent>
    </AlertDialog>
  )
}
