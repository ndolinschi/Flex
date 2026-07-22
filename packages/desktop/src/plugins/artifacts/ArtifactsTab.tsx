import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import {
  ExternalLink,
  FileSpreadsheet,
  FileText,
  Image,
  LayoutTemplate,
  Package,
  RefreshCw,
  Share2,
  Table2,
  X,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { EmptyState } from "../../components/molecules"
import {
  artifactsList,
  artifactsOpenExternal,
  artifactsPreviewCsv,
  toInvokeError,
  type Artifact,
  type CsvPreview,
} from "../../lib/tauri"
import type { SessionMeta } from "../../lib/types"
import { cn } from "../../lib/utils"
import type { ArtifactKind } from "../../lib/artifacts/types"

type ArtifactsTabProps = {
  active: boolean
  session: SessionMeta | undefined
}

/** Icon by artifact kind. */
const KindIcon = ({ kind, className }: { kind: ArtifactKind; className?: string }) => {
  switch (kind) {
    case "presentation":
      return <LayoutTemplate className={cn("shrink-0", className)} aria-hidden />
    case "spreadsheet":
      return <FileSpreadsheet className={cn("shrink-0", className)} aria-hidden />
    case "csv":
      return <Table2 className={cn("shrink-0", className)} aria-hidden />
    case "image":
      return <Image className={cn("shrink-0", className)} aria-hidden />
    case "diagram":
      return <Share2 className={cn("shrink-0", className)} aria-hidden />
    case "document":
      return <FileText className={cn("shrink-0", className)} aria-hidden />
    default:
      return <FileText className={cn("shrink-0", className)} aria-hidden />
  }
}

/** Short label for session affinity — uses the full id for now; callers
 *  pass the active session id to detect "This agent". */
const affinityLabel = (artifact: Artifact, activeSessionId: string | undefined): string => {
  if (!artifact.sessionId) return "Unknown agent"
  if (artifact.sessionId === activeSessionId) return "This agent"
  // Show last 8 chars of the session id as a short handle.
  return artifact.sessionId.slice(-8)
}

export const ArtifactsTab = ({ active, session }: ArtifactsTabProps) => {
  const projectKey = session?.cwd?.trim() ?? ""
  const activeSessionId = session?.id

  const [selected, setSelected] = useState<Artifact | null>(null)
  const [csvPreview, setCsvPreview] = useState<CsvPreview | null>(null)
  const [previewError, setPreviewError] = useState<string | null>(null)

  const {
    data: artifacts = [],
    isFetching,
    refetch,
  } = useQuery({
    queryKey: ["artifacts", projectKey],
    queryFn: () => artifactsList(projectKey),
    enabled: active && !!projectKey,
    staleTime: 15_000,
  })

  const openExternalMut = useMutation({
    mutationFn: (id: string) => artifactsOpenExternal(projectKey, id),
    onError: (err) => setPreviewError(toInvokeError(err)),
  })

  const handleSelect = async (artifact: Artifact) => {
    setSelected(artifact)
    setPreviewError(null)
    setCsvPreview(null)

    if (artifact.kind === "csv") {
      try {
        const preview = await artifactsPreviewCsv(projectKey, artifact.id)
        setCsvPreview(preview)
      } catch (err) {
        setPreviewError(toInvokeError(err))
      }
    }
  }

  const countLabel =
    artifacts.length === 0
      ? null
      : artifacts.length === 1
        ? "1 artifact"
        : `${artifacts.length} artifacts`

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Header — only rendered once we have artifacts (same pattern as DatabaseTab). */}
      {artifacts.length > 0 ? (
        <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 px-2.5">
          <span className="min-w-0 flex-1 truncate text-sm text-ink-muted">
            {countLabel}
          </span>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Refresh artifacts"
            title="Refresh artifacts"
            onClick={() => void refetch()}
            className={cn(
              "h-6 w-6 text-ink-muted hover:bg-fill-4 hover:text-ink",
              isFetching && "pointer-events-none",
            )}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", isFetching && "animate-spin")} />
          </Button>
        </div>
      ) : null}

      {!projectKey ? (
        <EmptyState
          className="min-h-0 flex-1"
          icon={<Package className="h-6 w-6" aria-hidden />}
          title="No project folder"
          description="Pick a working directory for this session to see its artifacts."
        />
      ) : artifacts.length === 0 ? (
        <EmptyState
          className="min-h-0 flex-1"
          icon={<Package className="h-6 w-6" aria-hidden />}
          title="No artifacts yet"
          description="Agent-created deliverables (reports, spreadsheets, presentations, diagrams, images) appear here automatically."
        />
      ) : (
        <div className="flex min-h-0 flex-1">
          {/* ── Left list — 180px sidebar (Terminal / Database pattern) ────── */}
          <aside className="flex w-[180px] shrink-0 flex-col border-r border-stroke-3">
            <ScrollArea className="min-h-0 flex-1 py-1.5">
              <ul>
                {artifacts.map((artifact) => {
                  const isActive = selected?.id === artifact.id
                  return (
                    <li key={artifact.id} className="group relative">
                      <Button
                        variant="ghost"
                        onClick={() => void handleSelect(artifact)}
                        className={cn(
                          "h-auto w-full flex-col items-start justify-start gap-0.5 rounded-none px-2.5 py-1.5 font-normal",
                          isActive
                            ? "bg-fill-2 hover:bg-fill-2"
                            : "hover:bg-fill-4",
                        )}
                      >
                        <div className="flex min-w-0 items-center gap-1.5">
                          <KindIcon
                            kind={artifact.kind}
                            className="h-3 w-3 text-icon-3"
                          />
                          <span className="min-w-0 truncate text-xs font-medium text-ink">
                            {artifact.title}
                          </span>
                        </div>
                        <span className="ml-[18px] min-w-0 max-w-full truncate text-[10px] text-ink-faint">
                          {artifact.relativePath}
                        </span>
                        <span className="ml-[18px] min-w-0 max-w-full truncate text-[10px] uppercase tracking-wide text-ink-faint">
                          {affinityLabel(artifact, activeSessionId)}
                        </span>
                      </Button>
                    </li>
                  )
                })}
              </ul>
            </ScrollArea>
          </aside>

          {/* ── Right preview pane ─────────────────────────────────────────── */}
          <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
            {!selected ? (
              <div className="flex flex-1 items-center justify-center px-4 text-center text-sm text-ink-muted">
                Select an artifact to preview.
              </div>
            ) : (
              <ArtifactPreview
                artifact={selected}
                csvPreview={csvPreview}
                error={previewError}
                isOpeningExternal={openExternalMut.isPending}
                onOpenExternal={() => openExternalMut.mutate(selected.id)}
                onDismissError={() => setPreviewError(null)}
              />
            )}
          </div>
        </div>
      )}
    </div>
  )
}

// ── Preview pane ──────────────────────────────────────────────────────────────

type ArtifactPreviewProps = {
  artifact: Artifact
  csvPreview: CsvPreview | null
  error: string | null
  isOpeningExternal: boolean
  onOpenExternal: () => void
  onDismissError: () => void
}

const ArtifactPreview = ({
  artifact,
  csvPreview,
  error,
  isOpeningExternal,
  onOpenExternal,
  onDismissError,
}: ArtifactPreviewProps) => {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Sub-header: title + kind badge + open-external */}
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-2 border-b border-stroke-3 px-2.5">
        <KindIcon kind={artifact.kind} className="h-3.5 w-3.5 text-icon-3" />
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-ink">
          {artifact.title}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="Open externally"
          title="Open externally"
          disabled={isOpeningExternal}
          onClick={onOpenExternal}
          className="h-6 w-6 text-ink-muted hover:bg-fill-4 hover:text-ink"
        >
          <ExternalLink className="h-3.5 w-3.5" aria-hidden />
        </Button>
      </div>

      {error ? (
        <div className="flex items-start gap-2 border-b border-stroke-3 bg-danger/5 px-2.5 py-2 text-xs text-danger">
          <span className="min-w-0 flex-1">{error}</span>
          <button
            onClick={onDismissError}
            className="shrink-0 opacity-60 hover:opacity-100"
            aria-label="Dismiss"
          >
            <X className="h-3 w-3" />
          </button>
        </div>
      ) : null}

      {/* Body */}
      <ScrollArea className="min-h-0 flex-1">
        {artifact.kind === "csv" ? (
          <CsvPreviewPanel preview={csvPreview} />
        ) : artifact.kind === "image" ? (
          <ImagePreviewPanel artifact={artifact} />
        ) : (
          <GenericPreviewPanel artifact={artifact} onOpen={onOpenExternal} />
        )}
      </ScrollArea>
    </div>
  )
}

// ── CSV table preview ─────────────────────────────────────────────────────────

const CsvPreviewPanel = ({ preview }: { preview: CsvPreview | null }) => {
  if (!preview) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-xs text-ink-muted">
        Loading preview…
      </div>
    )
  }
  if (preview.columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-xs text-ink-muted">
        No columns found.
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col">
      <ScrollArea className="min-h-0 flex-1">
        <Table className="w-max min-w-full text-xs">
          <TableHeader className="bg-fill-5">
            <TableRow>
              {preview.columns.map((col) => (
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
            {preview.rows.map((row, ri) => (
              <TableRow key={ri} className="odd:bg-fill-5/40">
                {row.map((cell, ci) => (
                  <TableCell
                    key={ci}
                    className="max-w-[12rem] truncate py-1 font-mono text-ink"
                    title={cell}
                  >
                    {cell}
                  </TableCell>
                ))}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </ScrollArea>
      {preview.truncated ? (
        <div className="shrink-0 border-t border-stroke-3 px-2.5 py-1.5 text-[10px] text-ink-faint">
          Showing first {preview.rowCount} rows (truncated)
        </div>
      ) : (
        <div className="shrink-0 border-t border-stroke-3 px-2.5 py-1.5 text-[10px] text-ink-faint">
          {preview.rowCount} {preview.rowCount === 1 ? "row" : "rows"}
        </div>
      )}
    </div>
  )
}

// ── Image preview ─────────────────────────────────────────────────────────────

const ImagePreviewPanel = ({ artifact }: { artifact: Artifact }) => {
  // Use Tauri's convertFileSrc to get a URL accessible from the webview.
  // This is the standard pattern for local files in Tauri apps.
  const [errored, setErrored] = useState(false)

  if (errored) {
    return (
      <GenericPreviewPanel
        artifact={artifact}
        onOpen={() => undefined}
        note="Image could not be loaded."
      />
    )
  }

  return (
    <div className="flex h-full items-center justify-center p-2">
      <img
        src={`https://asset.localhost/${artifact.relativePath}`}
        alt={artifact.title}
        className="max-h-full max-w-full object-contain"
        onError={() => setErrored(true)}
      />
    </div>
  )
}

// ── Generic "open externally" placeholder ─────────────────────────────────────

const GenericPreviewPanel = ({
  artifact,
  onOpen,
  note,
}: {
  artifact: Artifact
  onOpen: () => void
  note?: string
}) => (
  <EmptyState
    className="h-full flex-1 rounded-none border-none"
    title={artifact.title}
    description={
      note ??
      `${artifact.kind.charAt(0).toUpperCase() + artifact.kind.slice(1)} · ${artifact.relativePath}`
    }
    actionLabel="Open externally"
    onAction={onOpen}
  />
)
