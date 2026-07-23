import { useEffect, useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  ChevronLeft,
  ChevronRight,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Table2,
  Trash2,
  Unplug,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  Pagination,
  PaginationContent,
  PaginationItem,
} from "@/components/ui/pagination"
import { Separator } from "@/components/ui/separator"
import { ConfirmDialog, EmptyState, ErrorBanner, FormField } from "../../components/molecules"
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
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"

const PAGE_SIZE = 50

type DatabaseTabProps = {
  active: boolean
  session: SessionMeta | undefined
}

const ENGINE_OPTIONS: Array<{
  id: DbEngine
  label: string
  hint: string
  defaultTarget: (cwd: string | undefined) => string
  description: string
}> = [
  {
    id: "sqlite",
    label: "SQLite",
    hint: "C:\\path\\to\\data.db or /path/to/data.db",
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

export const DatabaseTab = ({ active, session }: DatabaseTabProps) => {
  const queryClient = useQueryClient()
  const projectKey = session?.cwd?.trim() ?? ""
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [schema, setSchema] = useState<string | null>(null)
  const [selectedTable, setSelectedTable] = useState<DbTableInfo | null>(null)
  const [sql, setSql] = useState("SELECT 1")
  const [result, setResult] = useState<DbQueryResult | null>(null)
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
      target: opt?.defaultTarget(session?.cwd) ?? "",
    }))
  }

  const { data: connections = [], isFetching } = useQuery({
    queryKey: ["db-connections", projectKey],
    queryFn: () => dbListConnections(projectKey),
    enabled: active && !!projectKey,
    staleTime: 10_000,
  })

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
    staleTime: 30_000,
  })

  const activeSchema = schema ?? schemas[0]?.name ?? null

  const {
    data: tables = [],
    isFetching: tablesFetching,
    refetch: refetchTables,
  } = useQuery({
    queryKey: ["db-tables", selectedId, activeSchema],
    queryFn: () => dbListTables(selectedId!, activeSchema ?? undefined),
    enabled: active && !!selectedId,
    staleTime: 15_000,
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
      {connections.length > 0 ? (
        <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 px-2.5">
          <span className="min-w-0 flex-1 truncate text-sm text-ink-muted">
            {connectionCountLabel}
          </span>
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Refresh tables" title="Refresh tables"
      onClick={() => void refetchTables()}
      className={cn(
        "text-ink-muted hover:bg-fill-4 hover:text-ink",
        "h-6 w-6",
      )}
    >
      <RefreshCw className={cn("h-3.5 w-3.5", isFetching && "animate-spin")} />
    </Button>
          <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label="Add connection" title="Add connection"
      onClick={openAddForm}
      className={cn(
        "text-ink-muted hover:bg-fill-4 hover:text-ink",
        "h-6 w-6",
      )}
    >
      <Plus className="h-3.5 w-3.5" />
    </Button>
        </div>
      ) : null}

      {error ? (
        <ErrorBanner
          message={error}
          className="rounded-none border-x-0 border-t-0 px-2.5 py-1.5 text-xs"
        />
      ) : null}

      {selectedId && schemas.length > 1 ? (
        <div className="flex shrink-0 gap-1 overflow-x-auto border-b border-stroke-3 px-2.5 py-1.5">
          {schemas.map((s) => (
            <Button
              key={s.name}
              variant="ghost"
              onClick={() => {
                setSchema(s.name)
                setSelectedTable(null)
              }}
              className={cn(
                "h-auto rounded-md px-2 py-0.5 text-xs font-normal",
                activeSchema === s.name
                  ? "bg-fill-2 text-ink hover:bg-fill-2"
                  : "text-ink-muted hover:bg-fill-4 hover:text-ink",
              )}
            >
              {s.name}
            </Button>
          ))}
        </div>
      ) : null}

      { !projectKey ? (
        <EmptyState
          className="min-h-0 flex-1"
          title="No project folder"
          description="Pick a working directory for this session to manage database connections for that project."
        />
      ) : isFetching && connections.length === 0 ? (
        <div className="flex min-h-0 flex-1 items-center justify-center gap-2 px-2.5 text-sm text-ink-muted">
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
          Loading connections…
        </div>
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
                  <Separator className="mx-2.5 my-1.5" />
                  <div className="flex h-6 shrink-0 items-center px-2.5 text-xs text-ink-muted">
                    <span>{tableCountLabel}</span>
                  </div>
                  {tablesFetching && tables.length === 0 ? (
                    <div className="flex items-center gap-1.5 px-2.5 py-2 text-xs text-ink-faint">
                      <Loader2 className="h-3 w-3 animate-spin" aria-hidden />
                      Loading tables…
                    </div>
                  ) : tables.length === 0 ? (
                    <p className="px-2.5 py-2 text-xs text-ink-faint">No tables</p>
                  ) : (
                    <ul>
                      {tables.map((t) => {
                        const key = `${t.schema}.${t.name}`
                        const isActive =
                          selectedTable?.schema === t.schema &&
                          selectedTable?.name === t.name
                        return (
                          <li key={key}>
                            <Button
                              variant="ghost"
                              onClick={() => void runPreview(t)}
                              className={cn(
                                "h-auto w-full justify-start gap-1.5 px-2.5 py-1.5 text-xs font-normal",
                                isActive
                                  ? "bg-fill-2 text-ink hover:bg-fill-2"
                                  : "text-ink-secondary hover:bg-fill-4 hover:text-ink",
                              )}
                            >
                              <Table2
                                className="h-3 w-3 shrink-0 text-icon-3"
                                aria-hidden
                              />
                              <span className="min-w-0 truncate font-mono">
                                {t.name}
                              </span>
                            </Button>
                          </li>
                        )
                      })}
                    </ul>
                  )}
                </>
              ) : null}
            </ScrollArea>
            {selectedId ? (
              <Button
                variant="ghost"
                className="h-auto justify-start gap-1.5 rounded-none border-t border-stroke-3 px-2.5 py-2 text-xs text-ink-muted font-normal hover:bg-fill-4 hover:text-ink"
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
              </Button>
            ) : null}
          </aside>

          <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
            {!selectedId ? (
              <EmptyState
                className="min-h-0 flex-1"
                title="Select a connection"
                description="Pick a connection on the left to browse tables and run SQL."
              />
            ) : (
              <>
                <div className="flex shrink-0 flex-col border-b border-stroke-3 px-2.5 py-2">
                  <div className="mb-1.5 flex shrink-0 justify-end">
                    <Button
                      size="sm"
                      variant="default"
                      className="h-6 gap-1 px-2 text-[11px]"
                      onClick={() => void runSql()}
                    >
                      <Play className="h-3 w-3" aria-hidden />
                      Run
                    </Button>
                  </div>
                  <Textarea
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
            <Input
              id="db-name"
              value={form.name}
              onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
              placeholder="Local app DB"
            />
          </FormField>
          <FormField label="Engine" htmlFor="db-engine">
            <Select
              items={ENGINE_OPTIONS.map((o) => ({ value: o.id, label: o.label }))}
              value={form.engine}
              onValueChange={(v) => {
                if (v == null) return
                setEngine(v as DbEngine)
              }}
            >
              <SelectTrigger id="db-engine" className="w-full" size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {ENGINE_OPTIONS.map((o) => (
                    <SelectItem key={o.id} value={o.id}>
                      {o.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </FormField>
          <FormField
            label={form.engine === "sqlite" ? "File path" : "Connection URL"}
            htmlFor="db-target"
            hint={engineMeta.hint}
          >
            <Input
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
            <ErrorBanner
              message={
                form.engine === "mysql"
                  ? "MySQL target must be a URL like mysql://user:pass@127.0.0.1:3306/dbname"
                  : "PostgreSQL target must be a URL like postgres://user:pass@127.0.0.1:5432/dbname"
              }
              className="py-1.5 text-xs"
            />
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
    <Button
      variant="ghost"
      disabled={busy}
      onClick={onOpen}
      className={cn(
        "h-auto w-full flex-col items-start justify-start gap-0.5 rounded-none px-2.5 py-1.5 font-normal",
        active ? "bg-fill-2 hover:bg-fill-2" : "hover:bg-fill-4",
      )}
    >
      <span className="truncate text-xs font-medium text-ink">{spec.name}</span>
      <span className="truncate text-[10px] uppercase tracking-wide text-ink-faint">
        {spec.engine}
      </span>
    </Button>
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={`Remove ${spec.name}`} title={`Remove ${spec.name}`}
      onClick={(e) => {
        e.stopPropagation()
        onDelete()
      }}
      className={cn(
        "text-ink-muted hover:bg-fill-4 hover:text-ink",
        "absolute right-1 top-1 h-5 w-5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
      )}
    >
      <Trash2 className="h-3 w-3" />
    </Button>
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
      <EmptyState
        className="flex-1 rounded-none border-none"
        title="Pick a table or run a query"
      />
    )
  }
  if (result.columns.length === 0) {
    return (
      <EmptyState
        className="flex-1 rounded-none border-none"
        title="No columns returned"
        description={`Query returned ${result.rowCount} rows.`}
      />
    )
  }

  const totalFetched = result.rows.length
  const start = page * pageSize
  const pageRows =
    kind === "query"
      ? result.rows.slice(start, start + pageSize)
      : result.rows
  const showingFrom = totalFetched === 0 ? 0 : start + 1
  const showingTo = start + pageRows.length

  const canPrev = page > 0
  const canNext =
    kind === "preview"
      ? pageRows.length >= pageSize
      : start + pageSize < totalFetched

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ScrollArea className="min-h-0 flex-1">
        <Table className="w-max min-w-full text-xs">
          <TableHeader className="bg-fill-5">
            <TableRow>
              {result.columns.map((col) => (
                <TableHead
                  key={col}
                  className="h-auto py-1.5 text-xs font-medium text-ink-secondary"
                >
                  {col}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {pageRows.map((row, ri) => (
              <TableRow key={start + ri} className="odd:bg-fill-5/40">
                {row.map((cell, ci) => (
                  <TableCell
                    key={ci}
                    className="max-w-[16rem] truncate py-1 font-mono text-ink"
                    title={cellLabel(cell)}
                  >
                    {cellLabel(cell)}
                  </TableCell>
                ))}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </ScrollArea>
      <div className="flex shrink-0 items-center gap-1 border-t border-stroke-3 px-2.5 py-1.5">
        <span className="min-w-0 flex-1 truncate text-[10px] text-ink-faint">
          {totalFetched === 0
            ? "0 rows"
            : kind === "query"
              ? `Showing ${showingFrom}–${showingTo} of ${totalFetched}`
              : `Showing ${showingFrom}–${showingTo}`}
          {result.truncated ? " (truncated)" : ""}
          {kind === "preview" && canNext ? "+" : ""}
        </span>
        <Pagination className="mx-0 w-auto">
          <PaginationContent className="gap-0">
            <PaginationItem>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Previous page"
                disabled={!canPrev}
                onClick={() => onPageChange(page - 1)}
                className="size-5"
              >
                <ChevronLeft aria-hidden />
              </Button>
            </PaginationItem>
            <PaginationItem>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Next page"
                disabled={!canNext}
                onClick={() => onPageChange(page + 1)}
                className="size-5"
              >
                <ChevronRight aria-hidden />
              </Button>
            </PaginationItem>
          </PaginationContent>
        </Pagination>
      </div>
    </div>
  )
}

const cellLabel = (cell: unknown): string => {
  if (cell === null || cell === undefined) return "NULL"
  if (typeof cell === "string") return cell
  return JSON.stringify(cell)
}
