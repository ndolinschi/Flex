import { useEffect, useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  ChevronLeft,
  ChevronRight,
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
  dbActiveConnection,
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

/** Rows per page for table preview (server) and query results (client). */
const PAGE_SIZE = 50

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
    return t.startsWith("postgres://") || t.startsWith("postgresql://")
  }
  if (engine === "mysql") {
    return t.startsWith("mysql://") || t.startsWith("mysql2://")
  }
  return target.trim().length > 0
}

/** Right-panel Database plugin — connections, schemas/tables, tabular data.
 * Connections are scoped to the active session's project cwd. */
export const DatabaseTab = ({ active, session }: DatabaseTabProps) => {
  const queryClient = useQueryClient()
  const projectKey = session?.cwd?.trim() ?? ""
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [schema, setSchema] = useState<string | null>(null)
  const [selectedTable, setSelectedTable] = useState<DbTableInfo | null>(null)
  const [sql, setSql] = useState("SELECT 1")
  const [result, setResult] = useState<DbQueryResult | null>(null)
  /** `preview` = server-paged table browse; `query` = client-paged Run result. */
  const [resultKind, setResultKind] = useState<"preview" | "query">("query")
  const [page, setPage] = useState(0)
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

  // Drop selection when the project cwd changes so another project's
  // connection never stays highlighted.
  useEffect(() => {
    setSelectedId(null)
    setSchema(null)
    setSelectedTable(null)
    setResult(null)
    setPage(0)
    setError(null)
  }, [projectKey])

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
    queryKey: ["db-connections", projectKey],
    queryFn: () => dbListConnections(projectKey),
    enabled: active && !!projectKey,
    staleTime: 10_000,
  })

  // Restore this project's last active connection after a session/cwd switch.
  useEffect(() => {
    if (!active || !projectKey) return
    let cancelled = false
    void dbActiveConnection(projectKey).then((spec) => {
      if (cancelled || !spec) return
      setSelectedId(spec.id)
    })
    return () => {
      cancelled = true
    }
  }, [active, projectKey])

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
      setPage(0)
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
        projectKey,
      }),
    onSuccess: (spec) => {
      setFormOpen(false)
      void queryClient.invalidateQueries({
        queryKey: ["db-connections", projectKey],
      })
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
        setPage(0)
      }
      void queryClient.invalidateQueries({
        queryKey: ["db-connections", projectKey],
      })
    },
    onError: (err) => setError(toInvokeError(err)),
  })

  const loadPreviewPage = async (table: DbTableInfo, nextPage: number) => {
    if (!selectedId) return
    const offset = nextPage * PAGE_SIZE
    setError(null)
    try {
      const preview = await dbPreviewTable(
        selectedId,
        table.schema,
        table.name,
        PAGE_SIZE,
        offset,
      )
      setSelectedTable(table)
      setResultKind("preview")
      setPage(nextPage)
      setResult(preview)
      setSql(
        offset === 0
          ? `SELECT * FROM ${qualify(table)} LIMIT ${PAGE_SIZE}`
          : `SELECT * FROM ${qualify(table)} LIMIT ${PAGE_SIZE} OFFSET ${offset}`,
      )
    } catch (err) {
      setError(toInvokeError(err))
    }
  }

  const runPreview = async (table: DbTableInfo) => {
    await loadPreviewPage(table, 0)
  }

  const runSql = async () => {
    if (!selectedId) return
    setError(null)
    setSelectedTable(null)
    try {
      const out = await dbQuery(selectedId, sql)
      setResultKind("query")
      setPage(0)
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

  const connectionCountLabel =
    connections.length === 1
      ? "1 connection"
      : `${connections.length} connections`

  const tableCountLabel =
    tables.length === 0
      ? "Tables"
      : tables.length === 1
        ? "1 Table"
        : `${tables.length} Tables`

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Tab strip already labels the panel — don't repeat "Database" here.
          Empty state owns the Add CTA; chrome only appears once there are
          connections (count + refresh/add), so stacked headers stay balanced. */}
      {connections.length > 0 ? (
        <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-4">
          <span className="min-w-0 flex-1 truncate text-sm text-ink-muted">
            {connectionCountLabel}
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
        <p className="border-b border-stroke-3 bg-danger-subtle px-3 py-1.5 text-xs text-danger">
          {error}
        </p>
      ) : null}

      {selectedId && schemas.length > 1 ? (
        <div className="flex shrink-0 gap-1 overflow-x-auto border-b border-stroke-3 px-3 py-1">
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

      { !projectKey ? (
        <EmptyState
          className="min-h-0 flex-1"
          title="No project folder"
          description="Pick a working directory for this session to manage database connections for that project."
        />
      ) : connections.length === 0 ? (
        <EmptyState
          className="min-h-0 flex-1"
          title="No database connections"
          description="Connect SQLite, PostgreSQL, or MySQL to browse schemas, tables, and rows. Connections are saved per project."
          actionLabel="Add connection"
          onAction={openAddForm}
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          {/* Single sidebar (Terminal pattern) — connections + tables.
              The old 3-col layout left ~36px for SQL/Run at default panel width. */}
          <aside className="flex w-[180px] shrink-0 flex-col border-r border-stroke-3">
            <ScrollArea className="min-h-0 flex-1 py-1.5">
              <ul>
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

              {selectedId ? (
                <>
                  <div className="mx-2 my-1.5 border-t border-stroke-3" />
                  <div className="flex h-6 shrink-0 items-center px-2 text-xs text-ink-muted">
                    <span>{tableCountLabel}</span>
                  </div>
                  {tables.length === 0 ? (
                    <p className="px-2 py-2 text-xs text-ink-faint">No tables</p>
                  ) : (
                    <ul>
                      {tables.map((t) => {
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
                      })}
                    </ul>
                  )}
                </>
              ) : null}
            </ScrollArea>
            {selectedId ? (
              <button
                type="button"
                className="flex items-center gap-1.5 border-t border-stroke-3 px-2 py-2 text-xs text-ink-muted hover:text-ink"
                onClick={() => {
                  void dbDisconnect(selectedId)
                  setSelectedId(null)
                  setSelectedTable(null)
                  setResult(null)
                  setPage(0)
                }}
              >
                <Unplug className="h-3 w-3" aria-hidden />
                Disconnect
              </button>
            ) : null}
          </aside>

          <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
            {!selectedId ? (
              <div className="flex flex-1 items-center justify-center px-4 text-center text-sm text-ink-muted">
                Select a connection to browse tables.
              </div>
            ) : (
              <>
                <div className="flex shrink-0 flex-col border-b border-stroke-3 p-2">
                  <div className="mb-1.5 flex shrink-0 justify-end">
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => void runSql()}
                    >
                      <Play className="h-3 w-3" aria-hidden />
                      Run
                    </Button>
                  </div>
                  <TextArea
                    value={sql}
                    onChange={(e) => setSql(e.target.value)}
                    rows={3}
                    className="max-h-28 resize-y font-mono text-xs"
                    aria-label="SQL query"
                    onKeyDown={(e) => {
                      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                        e.preventDefault()
                        void runSql()
                      }
                    }}
                  />
                </div>
                <ResultGrid
                  result={result}
                  kind={resultKind}
                  page={page}
                  pageSize={PAGE_SIZE}
                  onPageChange={(next) => {
                    if (resultKind === "preview" && selectedTable) {
                      void loadPreviewPage(selectedTable, next)
                      return
                    }
                    setPage(next)
                  }}
                />
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

const ResultGrid = ({
  result,
  kind,
  page,
  pageSize,
  onPageChange,
}: {
  result: DbQueryResult | null
  kind: "preview" | "query"
  page: number
  pageSize: number
  onPageChange: (page: number) => void
}) => {
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

  // Query: page client-side over the fetched batch (backend caps at 500).
  // Preview: each `result` is already one server page.
  const totalFetched = result.rows.length
  const start = page * pageSize
  const pageRows =
    kind === "query"
      ? result.rows.slice(start, start + pageSize)
      : result.rows
  const showingFrom = totalFetched === 0 ? 0 : start + 1
  const showingTo = start + pageRows.length

  const canPrev = page > 0
  // Preview: full page ⇒ likely more rows; query: more pages in the batch.
  const canNext =
    kind === "preview"
      ? pageRows.length >= pageSize
      : start + pageSize < totalFetched

  return (
    <div className="flex min-h-0 flex-1 flex-col">
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
            {pageRows.map((row, ri) => (
              <tr key={start + ri} className="odd:bg-fill-5/40">
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
      </ScrollArea>
      <div className="flex shrink-0 items-center gap-1 border-t border-stroke-3 px-2 py-1">
        <span className="min-w-0 flex-1 truncate text-[10px] text-ink-faint">
          {totalFetched === 0
            ? "0 rows"
            : kind === "query"
              ? `Showing ${showingFrom}–${showingTo} of ${totalFetched}`
              : `Showing ${showingFrom}–${showingTo}`}
          {result.truncated ? " (truncated)" : ""}
          {kind === "preview" && canNext ? "+" : ""}
        </span>
        <IconButton
          label="Previous page"
          className="h-5 w-5"
          disabled={!canPrev}
          onClick={() => onPageChange(page - 1)}
        >
          <ChevronLeft className="h-3 w-3" aria-hidden />
        </IconButton>
        <IconButton
          label="Next page"
          className="h-5 w-5"
          disabled={!canNext}
          onClick={() => onPageChange(page + 1)}
        >
          <ChevronRight className="h-3 w-3" aria-hidden />
        </IconButton>
      </div>
    </div>
  )
}

const cellLabel = (cell: unknown): string => {
  if (cell === null || cell === undefined) return "NULL"
  if (typeof cell === "string") return cell
  return JSON.stringify(cell)
}
