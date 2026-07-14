import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  Play,
  Plus,
  RefreshCw,
  Table2,
  Trash2,
  Unplug,
} from "lucide-react"
import { Button, IconButton, ScrollArea, TextArea, TextInput } from "../../components/atoms"
import { ConfirmDialog, EmptyState, FormField } from "../../components/molecules"
import {
  dbConnect,
  dbDisconnect,
  dbListConnections,
  dbListSchemas,
  dbListTables,
  dbPreviewTable,
  dbQuery,
  dbRemoveConnection,
  dbUpsertConnection,
  toInvokeError,
  type DbConnectionSpec,
  type DbEngine,
  type DbQueryResult,
  type DbTableInfo,
} from "../../lib/tauri"
import type { SessionMeta } from "../../lib/types"
import { cn } from "../../lib/utils"

type DatabaseTabProps = {
  active: boolean
  session: SessionMeta | undefined
}

const ENGINE_OPTIONS: Array<{
  id: DbEngine
  label: string
  /** Short placeholder shown in the Target input. */
  hint: string
  /** Default target when switching to this engine (empty = clear). */
  defaultTarget: (cwd: string | undefined) => string
  /** Dialog blurb for this engine. */
  description: string
}> = [
  {
    id: "sqlite",
    label: "SQLite",
    hint: "C:\\path\\to\\data.db  or  /path/to/data.db",
    defaultTarget: (cwd) => (cwd ? `${cwd}/data.db` : ""),
    description: "Absolute path to a .db / .sqlite file on disk.",
  },
  {
    id: "postgres",
    label: "PostgreSQL",
    hint: "postgres://user:pass@127.0.0.1:5432/dbname",
    defaultTarget: () => "postgres://user:pass@127.0.0.1:5432/dbname",
    description:
      "Connection URL. For Docker Compose Postgres on localhost use host 127.0.0.1 and the published port.",
  },
  {
    id: "mysql",
    label: "MySQL",
    hint: "mysql://flexuser:pass@127.0.0.1:3306/flexdb",
    defaultTarget: () => "mysql://flexuser:pass@127.0.0.1:3306/flexdb",
    description:
      "Connection URL. For Docker Compose MySQL on localhost: mysql://USER:PASSWORD@127.0.0.1:PORT/DATABASE (use 127.0.0.1, not the container name).",
  },
]

const isUrlLikeTarget = (engine: DbEngine, target: string): boolean => {
  const t = target.trim().toLowerCase()
  if (engine === "postgres") {
    return (
      t.startsWith("postgres://") ||
      t.startsWith("postgresql://")
    )
  }
  if (engine === "mysql") {
    return t.startsWith("mysql://") || t.startsWith("mysql2://")
  }
  return target.trim().length > 0
}

/** Right-panel Database plugin — connections, schemas/tables, tabular data. */
export const DatabaseTab = ({ active, session }: DatabaseTabProps) => {
  const queryClient = useQueryClient()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [schema, setSchema] = useState<string | null>(null)
  const [selectedTable, setSelectedTable] = useState<DbTableInfo | null>(null)
  const [sql, setSql] = useState("SELECT 1")
  const [result, setResult] = useState<DbQueryResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [formOpen, setFormOpen] = useState(false)
  const [form, setForm] = useState<{
    name: string
    engine: DbEngine
    target: string
  }>({
    name: "",
    engine: "sqlite",
    target: session?.cwd ? `${session.cwd}/data.db` : "",
  })
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null)

  const openAddForm = () => {
    setForm({
      name: "",
      engine: "sqlite",
      target: ENGINE_OPTIONS[0].defaultTarget(session?.cwd),
    })
    setFormOpen(true)
  }

  const setEngine = (engine: DbEngine) => {
    const opt = ENGINE_OPTIONS.find((e) => e.id === engine)
    setForm((f) => ({
      ...f,
      engine,
      // Always swap the target when the engine changes so a leftover SQLite
      // path never sits under MySQL/Postgres (the bug that made docker-compose
      // URLs look "broken" in the form).
      target: opt?.defaultTarget(session?.cwd) ?? "",
    }))
  }

  const { data: connections = [], isFetching } = useQuery({
    queryKey: ["db-connections"],
    queryFn: dbListConnections,
    enabled: active,
    staleTime: 10_000,
  })

  const { data: schemas = [] } = useQuery({
    queryKey: ["db-schemas", selectedId],
    queryFn: () => dbListSchemas(selectedId!),
    enabled: active && !!selectedId,
  })

  const activeSchema = schema ?? schemas[0]?.name ?? null

  const { data: tables = [], refetch: refetchTables } = useQuery({
    queryKey: ["db-tables", selectedId, activeSchema],
    queryFn: () => dbListTables(selectedId!, activeSchema ?? undefined),
    enabled: active && !!selectedId,
  })

  const connectMut = useMutation({
    mutationFn: (id: string) => dbConnect(id),
    onSuccess: (spec) => {
      setSelectedId(spec.id)
      setError(null)
      setSelectedTable(null)
      setResult(null)
      void queryClient.invalidateQueries({ queryKey: ["db-schemas", spec.id] })
      void queryClient.invalidateQueries({ queryKey: ["db-tables", spec.id] })
    },
    onError: (err) => setError(toInvokeError(err)),
  })

  const saveMut = useMutation({
    mutationFn: () =>
      dbUpsertConnection({
        id: "",
        name: form.name,
        engine: form.engine,
        target: form.target,
      }),
    onSuccess: (spec) => {
      setFormOpen(false)
      void queryClient.invalidateQueries({ queryKey: ["db-connections"] })
      connectMut.mutate(spec.id)
    },
    onError: (err) => setError(toInvokeError(err)),
  })

  const removeMut = useMutation({
    mutationFn: (id: string) => dbRemoveConnection(id),
    onSuccess: (_, id) => {
      if (selectedId === id) {
        setSelectedId(null)
        setSelectedTable(null)
        setResult(null)
      }
      void queryClient.invalidateQueries({ queryKey: ["db-connections"] })
    },
    onError: (err) => setError(toInvokeError(err)),
  })

  const runPreview = async (table: DbTableInfo) => {
    if (!selectedId) return
    setSelectedTable(table)
    setError(null)
    try {
      const preview = await dbPreviewTable(
        selectedId,
        table.schema,
        table.name,
        100,
      )
      setResult(preview)
      setSql(`SELECT * FROM ${qualify(table)} LIMIT 100`)
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  const runSql = async () => {
    if (!selectedId) return
    setError(null)
    try {
      const out = await dbQuery(selectedId, sql)
      setResult(out)
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  const engineMeta = useMemo(
    () => ENGINE_OPTIONS.find((e) => e.id === form.engine) ?? ENGINE_OPTIONS[0],
    [form.engine],
  )

  const targetOk = isUrlLikeTarget(form.engine, form.target)
  const canConnect =
    form.name.trim().length > 0 && form.target.trim().length > 0 && targetOk

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Tab strip already labels the panel — don't repeat "Database" here.
          Empty state owns the Add CTA; chrome only appears once there are
          connections (count + refresh/add), so stacked headers stay balanced. */}
      {connections.length > 0 ? (
        <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 px-2">
          <span className="min-w-0 flex-1 truncate text-sm text-ink-muted">
            {`${connections.length} connection${connections.length === 1 ? "" : "s"}`}
          </span>
          <IconButton
            label="Refresh tables"
            className="h-6 w-6"
            onClick={() => void refetchTables()}
          >
            <RefreshCw className={cn("h-3 w-3", isFetching && "animate-spin")} />
          </IconButton>
          <IconButton
            label="Add connection"
            className="h-6 w-6"
            onClick={openAddForm}
          >
            <Plus className="h-3.5 w-3.5" />
          </IconButton>
        </div>
      ) : null}

      {error ? (
        <p className="border-b border-stroke-3 bg-danger-subtle px-2 py-1.5 text-xs text-danger">
          {error}
        </p>
      ) : null}

      {connections.length === 0 ? (
        <EmptyState
          className="min-h-0 flex-1"
          title="No database connections"
          description="Connect SQLite, PostgreSQL, or MySQL to browse schemas, tables, and rows."
          actionLabel="Add connection"
          onAction={openAddForm}
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          <aside className="flex w-[11.5rem] shrink-0 flex-col border-r border-stroke-3">
            <ScrollArea className="min-h-0 flex-1">
              <ul className="py-1">
                {connections.map((c) => (
                  <ConnectionRow
                    key={c.id}
                    spec={c}
                    active={c.id === selectedId}
                    busy={connectMut.isPending}
                    onOpen={() => connectMut.mutate(c.id)}
                    onDelete={() => setConfirmDelete(c.id)}
                  />
                ))}
              </ul>
            </ScrollArea>
            {selectedId ? (
              <button
                type="button"
                className="flex items-center gap-1.5 border-t border-stroke-3 px-2 py-2 text-xs text-ink-muted hover:text-ink"
                onClick={() => {
                  void dbDisconnect(selectedId)
                  setSelectedId(null)
                  setResult(null)
                }}
              >
                <Unplug className="h-3 w-3" aria-hidden />
                Disconnect
              </button>
            ) : null}
          </aside>

          <div className="flex min-w-0 flex-1 flex-col">
            {!selectedId ? (
              <div className="flex flex-1 items-center justify-center px-4 text-center text-sm text-ink-muted">
                Select a connection to browse tables.
              </div>
            ) : (
              <>
                {schemas.length > 1 ? (
                  <div className="flex shrink-0 gap-1 overflow-x-auto border-b border-stroke-3 px-2 py-1.5">
                    {schemas.map((s) => (
                      <button
                        key={s.name}
                        type="button"
                        onClick={() => {
                          setSchema(s.name)
                          setSelectedTable(null)
                        }}
                        className={cn(
                          "rounded-md px-2 py-0.5 text-xs",
                          activeSchema === s.name
                            ? "bg-fill-3 text-ink"
                            : "text-ink-muted hover:bg-fill-3/60 hover:text-ink",
                        )}
                      >
                        {s.name}
                      </button>
                    ))}
                  </div>
                ) : null}

                <div className="flex min-h-0 flex-1">
                  <ScrollArea className="w-40 shrink-0 border-r border-stroke-3">
                    <ul className="py-1">
                      {tables.length === 0 ? (
                        <li className="px-2 py-3 text-xs text-ink-faint">
                          No tables
                        </li>
                      ) : (
                        tables.map((t) => {
                          const key = `${t.schema}.${t.name}`
                          const isActive =
                            selectedTable?.schema === t.schema &&
                            selectedTable?.name === t.name
                          return (
                            <li key={key}>
                              <button
                                type="button"
                                onClick={() => void runPreview(t)}
                                className={cn(
                                  "flex w-full items-center gap-1.5 px-2 py-1.5 text-left text-xs",
                                  isActive
                                    ? "bg-fill-3 text-ink"
                                    : "text-ink-secondary hover:bg-fill-3/60 hover:text-ink",
                                )}
                              >
                                <Table2
                                  className="h-3 w-3 shrink-0 text-icon-3"
                                  aria-hidden
                                />
                                <span className="min-w-0 truncate font-mono">
                                  {t.name}
                                </span>
                              </button>
                            </li>
                          )
                        })
                      )}
                    </ul>
                  </ScrollArea>

                  <div className="flex min-w-0 flex-1 flex-col">
                    <div className="flex shrink-0 flex-col gap-1.5 border-b border-stroke-3 p-2">
                      <TextArea
                        value={sql}
                        onChange={(e) => setSql(e.target.value)}
                        rows={3}
                        className="resize-y font-mono text-xs"
                        aria-label="SQL query"
                      />
                      <div className="flex justify-end">
                        <Button
                          size="sm"
                          variant="primary"
                          onClick={() => void runSql()}
                        >
                          <Play className="h-3 w-3" aria-hidden />
                          Run
                        </Button>
                      </div>
                    </div>
                    <ResultGrid result={result} />
                  </div>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      <ConfirmDialog
        open={formOpen}
        title="Add database connection"
        description={engineMeta.description}
        confirmLabel="Connect"
        isLoading={saveMut.isPending}
        confirmDisabled={!canConnect}
        onCancel={() => setFormOpen(false)}
        onConfirm={() => saveMut.mutate()}
      >
        <div className="flex flex-col gap-3">
          <FormField label="Name" htmlFor="db-name">
            <TextInput
              id="db-name"
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="Local app DB"
            />
          </FormField>
          <FormField label="Engine" htmlFor="db-engine">
            <select
              id="db-engine"
              value={form.engine}
              onChange={(e) => setEngine(e.target.value as DbEngine)}
              className="h-9 w-full rounded-md border border-stroke-2 bg-bg px-2 text-sm text-ink"
            >
              {ENGINE_OPTIONS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </FormField>
          <FormField
            label={form.engine === "sqlite" ? "File path" : "Connection URL"}
            htmlFor="db-target"
            hint={engineMeta.hint}
          >
            <TextInput
              id="db-target"
              value={form.target}
              onChange={(e) =>
                setForm((f) => ({ ...f, target: e.target.value }))
              }
              placeholder={engineMeta.hint}
              className="font-mono text-xs"
            />
          </FormField>
          {form.target.trim() && !targetOk ? (
            <p className="text-xs text-danger">
              {form.engine === "mysql"
                ? "MySQL target must be a URL like mysql://user:pass@127.0.0.1:3306/dbname"
                : "PostgreSQL target must be a URL like postgres://user:pass@127.0.0.1:5432/dbname"}
            </p>
          ) : null}
        </div>
      </ConfirmDialog>

      <ConfirmDialog
        open={!!confirmDelete}
        title="Remove connection?"
        description="This only removes the saved connection — it does not delete database data."
        confirmLabel="Remove"
        danger
        onCancel={() => setConfirmDelete(null)}
        onConfirm={() => {
          if (confirmDelete) removeMut.mutate(confirmDelete)
          setConfirmDelete(null)
        }}
      />
    </div>
  )
}

const qualify = (t: DbTableInfo): string =>
  t.schema && t.schema !== "main" ? `${t.schema}.${t.name}` : t.name

const ConnectionRow = ({
  spec,
  active,
  busy,
  onOpen,
  onDelete,
}: {
  spec: DbConnectionSpec
  active: boolean
  busy: boolean
  onOpen: () => void
  onDelete: () => void
}) => (
  <li className="group relative">
    <button
      type="button"
      disabled={busy}
      onClick={onOpen}
      className={cn(
        "flex w-full flex-col gap-0.5 px-2 py-1.5 text-left",
        active ? "bg-fill-3" : "hover:bg-fill-3/60",
      )}
    >
      <span className="truncate text-xs font-medium text-ink">{spec.name}</span>
      <span className="truncate text-[10px] uppercase tracking-wide text-ink-faint">
        {spec.engine}
      </span>
    </button>
    <IconButton
      label={`Remove ${spec.name}`}
      className="absolute right-1 top-1 h-5 w-5 opacity-0 group-hover:opacity-100"
      onClick={(e) => {
        e.stopPropagation()
        onDelete()
      }}
    >
      <Trash2 className="h-3 w-3" />
    </IconButton>
  </li>
)

const ResultGrid = ({ result }: { result: DbQueryResult | null }) => {
  if (!result) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-center text-sm text-ink-muted">
        Pick a table or run a query.
      </div>
    )
  }
  if (result.columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-sm text-ink-muted">
        Query returned no columns ({result.rowCount} rows).
      </div>
    )
  }
  return (
    <ScrollArea className="min-h-0 flex-1">
      <table className="w-max min-w-full border-collapse text-left text-xs">
        <thead className="sticky top-0 bg-fill-5">
          <tr>
            {result.columns.map((col) => (
              <th
                key={col}
                className="border-b border-stroke-3 px-2 py-1.5 font-medium text-ink-secondary"
              >
                {col}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {result.rows.map((row, ri) => (
            <tr key={ri} className="odd:bg-fill-5/40">
              {row.map((cell, ci) => (
                <td
                  key={ci}
                  className="max-w-[16rem] truncate border-b border-stroke-3/60 px-2 py-1 font-mono text-ink"
                  title={cellLabel(cell)}
                >
                  {cellLabel(cell)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="px-2 py-1.5 text-[10px] text-ink-faint">
        {result.rowCount} row{result.rowCount === 1 ? "" : "s"}
        {result.truncated ? " (truncated)" : ""}
      </p>
    </ScrollArea>
  )
}

const cellLabel = (cell: unknown): string => {
  if (cell === null || cell === undefined) return "NULL"
  if (typeof cell === "string") return cell
  return JSON.stringify(cell)
}
