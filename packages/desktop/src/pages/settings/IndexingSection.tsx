import { useEffect, useMemo, useState } from "react"
import { RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Spinner as ButtonSpinner } from "@/components/ui/spinner"
import { Spinner } from "../../components/atoms"
import { Switch } from "@/components/ui/switch"
import { ErrorBanner, SettingsCard, SettingRow } from "../../components/molecules"
import { useProviderConfig } from "../../hooks/useProviderConfig"
import { useSessions } from "../../hooks/useSessions"
import { indexRebuild, indexStatus, toInvokeError } from "../../lib/tauri"
import type { IndexStatus } from "../../lib/types"
import { basename } from "../../lib/utils"

type RowState = {
  cwd: string
  label: string
  status: IndexStatus | null
  loading: boolean
  rebuilding: boolean
  error: string | null
}

/** Settings → Indexing: per-repo status, plugin toggle, auto-context, rebuild. */
export const IndexingContent = () => {
  const { config, isLoading, save } = useProviderConfig()
  const { sessions } = useSessions()
  const [busyToggle, setBusyToggle] = useState<
    "index" | "autoContext" | "autoUpdateIndex" | null
  >(null)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [rows, setRows] = useState<RowState[]>([])

  const plugins = config?.plugins
  const repos = useMemo(() => {
    const seen = new Set<string>()
    const out: Array<{ cwd: string; label: string }> = []
    for (const session of sessions) {
      if (session.parent_id) continue
      const cwd = session.cwd?.trim()
      if (!cwd || seen.has(cwd)) continue
      seen.add(cwd)
      out.push({ cwd, label: basename(cwd) || cwd })
    }
    out.sort((a, b) => a.label.localeCompare(b.label))
    return out
  }, [sessions])

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      if (!plugins?.index || repos.length === 0) {
        setRows([])
        return
      }
      setRows(
        repos.map((r) => ({
          cwd: r.cwd,
          label: r.label,
          status: null,
          loading: true,
          rebuilding: false,
          error: null,
        })),
      )
      const next = await Promise.all(
        repos.map(async (r) => {
          try {
            const status = await indexStatus(r.cwd)
            return {
              cwd: r.cwd,
              label: r.label,
              status,
              loading: false,
              rebuilding: false,
              error: null,
            } satisfies RowState
          } catch (err) {
            return {
              cwd: r.cwd,
              label: r.label,
              status: null,
              loading: false,
              rebuilding: false,
              error: toInvokeError(err),
            } satisfies RowState
          }
        }),
      )
      if (!cancelled) setRows(next)
    }
    void load()
    return () => {
      cancelled = true
    }
  }, [plugins?.index, repos])

  const handleSavePlugins = async (
    patch: Partial<{
      index: boolean
      autoContext: boolean
      autoUpdateIndex: boolean
    }>,
    key: "index" | "autoContext" | "autoUpdateIndex",
  ) => {
    if (!config || !plugins || busyToggle) return
    setSaveError(null)
    setBusyToggle(key)
    try {
      await save({
        preferredProvider: config.preferredProvider ?? "",
        baseUrl: config.baseUrl,
        defaultModel: config.defaultModel,
        fallbackModels: config.fallbackModels,
        defaultIsolation:
          typeof config.defaultIsolation === "string"
            ? config.defaultIsolation
            : undefined,
        plugins: {
          ...plugins,
          autoContext: plugins.autoContext ?? false,
          autoUpdateIndex: plugins.autoUpdateIndex ?? false,
          ...patch,
        },
      })
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyToggle(null)
    }
  }

  const handleRebuild = async (cwd: string) => {
    setRows((prev) =>
      prev.map((row) =>
        row.cwd === cwd
          ? { ...row, rebuilding: true, error: null }
          : row,
      ),
    )
    try {
      const result = await indexRebuild(cwd)
      setRows((prev) =>
        prev.map((row) =>
          row.cwd === cwd
            ? {
                ...row,
                status: result.status,
                rebuilding: false,
                loading: false,
                error: null,
              }
            : row,
        ),
      )
    } catch (err) {
      setRows((prev) =>
        prev.map((row) =>
          row.cwd === cwd
            ? {
                ...row,
                rebuilding: false,
                error: toInvokeError(err),
              }
            : row,
        ),
      )
    }
  }

  return (
    <div className="flex flex-col gap-3">
      {saveError ? (
        <ErrorBanner message={saveError} onDismiss={() => setSaveError(null)} />
      ) : null}

      <SettingsCard label="Code index">
        <SettingRow
          rowId="indexing-enabled"
          title="Enable code index"
          description="SearchCode, FindSymbol, and RepoMap tools. Index files live in app data, never inside the repo."
          first
        >
          {isLoading || !plugins ? (
            <Spinner size="sm" />
          ) : (
            <Switch
              checked={plugins.index}
              onCheckedChange={(on) => void handleSavePlugins({ index: on }, "index")}
              aria-label="Toggle code index plugin"
              title="Toggle code index plugin"
              disabled={busyToggle !== null}
            />
          )}
        </SettingRow>
        <SettingRow
          rowId="indexing-auto-update"
          title="Update index on search"
          description="Rescan the repo before SearchCode, FindSymbol, or RepoMap. Off by default so a warm index is reused across chats; use Rebuild below when you want a refresh. Requires the code index plugin."
        >
          {isLoading || !plugins ? (
            <Spinner size="sm" />
          ) : (
            <Switch
              checked={!!plugins.autoUpdateIndex && plugins.index}
              onCheckedChange={(on) =>
                void handleSavePlugins({ autoUpdateIndex: on }, "autoUpdateIndex")
              }
              aria-label="Toggle update index on search"
              title="Toggle update index on search"
              disabled={busyToggle !== null || !plugins.index}
            />
          )}
        </SettingRow>
        <SettingRow
          rowId="indexing-auto-context"
          title="Auto-context"
          description="On turn start, inject top indexed snippets matching the prompt into the first model call. Default off. Requires the code index plugin."
        >
          {isLoading || !plugins ? (
            <Spinner size="sm" />
          ) : (
            <Switch
              checked={!!plugins.autoContext && plugins.index}
              onCheckedChange={(on) =>
                void handleSavePlugins({ autoContext: on }, "autoContext")
              }
              aria-label="Toggle auto-context"
              title="Toggle auto-context"
              disabled={busyToggle !== null || !plugins.index}
            />
          )}
        </SettingRow>
      </SettingsCard>

      <SettingsCard label="Repositories">
        {!plugins?.index ? (
          <p className="px-3.5 py-3 text-sm text-ink-muted">
            Enable the code index plugin to see per-repo status.
          </p>
        ) : repos.length === 0 ? (
          <p className="px-3.5 py-3 text-sm text-ink-muted">
            Open a folder and start a session to index a repository.
          </p>
        ) : (
          rows.map((row, i) => (
            <SettingRow
              key={row.cwd}
              rowId={`indexing-repo-${i}`}
              title={row.label}
              description={
                row.error
                  ? row.error
                  : row.loading
                    ? "Loading status…"
                    : row.status?.ready
                      ? `${row.status.fileCount} files · ${row.status.symbolCount} symbols`
                      : "Not indexed yet"
              }
              first={i === 0}
            >
              <div className="flex items-center gap-2">
                {row.status?.ready ? (
                  <span className="text-xs text-green">indexed</span>
                ) : null}
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={row.rebuilding || row.loading}
                  onClick={() => void handleRebuild(row.cwd)}
                  aria-label={`Rebuild index for ${row.label}`}
                >
                  {row.rebuilding ? <ButtonSpinner data-icon="inline-start" /> : null}
                  <RefreshCw className="h-3 w-3" aria-hidden />
                  Rebuild
                </Button>
              </div>
            </SettingRow>
          ))
        )}
      </SettingsCard>
    </div>
  )
}
