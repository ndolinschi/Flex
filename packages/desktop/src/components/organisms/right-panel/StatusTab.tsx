import { Activity } from "@/components/icons"
import { useModels } from "../../../hooks/useModels"
import { cacheTotalsFromModelUsage } from "../../../lib/modelUsage"
import { sessionLabel, type SessionMeta } from "../../../lib/types"
import { cn, formatTokens } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"

const CONTEXT_BUDGET_TOKENS = 200_000
/** Stable empty — inline `?? []` in a Zustand selector re-renders forever. */
const EMPTY_QUEUE: string[] = []

type StatusTabProps = {
  session: SessionMeta
  active: boolean
}

const Metric = ({ label, value }: { label: string; value: string }) => (
  <div className="flex min-w-0 flex-1 flex-col gap-0.5">
    <span className="text-[10px] font-medium uppercase tracking-wide text-ink-faint">
      {label}
    </span>
    <span className="truncate text-base font-medium text-ink [font-variant-numeric:tabular-nums]">
      {value}
    </span>
  </div>
)

const DetailRow = ({ label, value }: { label: string; value: string }) => (
  <div className="flex items-baseline justify-between gap-4 py-0.5">
    <span className="shrink-0 text-sm text-ink-muted">{label}</span>
    <span className="min-w-0 truncate text-right text-sm text-ink [font-variant-numeric:tabular-nums]">
      {value}
    </span>
  </div>
)

/** OpenCode-style session status: model, context, tokens, queue, per-model. */
export const StatusTab = ({ session, active }: StatusTabProps) => {
  const sessionId = session.id
  const streaming = useAppStore((s) => !!s.streamingSessions[sessionId])
  const errorSeen = useAppStore((s) => s.sessionErrorSeen[sessionId] ?? 0)
  const summary = useAppStore((s) => s.lastTurnSummary[sessionId])
  const usage = useAppStore((s) => s.lastTurnUsage[sessionId])
  const totals = useAppStore((s) => s.sessionTotals[sessionId])
  const modelUsage = useAppStore((s) => s.modelUsageBySession[sessionId])
  const lastModel = useAppStore((s) => s.lastModelBySession[sessionId])
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const queue = useAppStore(
    (s) => s.messageQueueBySession[sessionId] ?? EMPTY_QUEUE,
  )
  const compaction = useAppStore((s) => s.lastCompactionBySession[sessionId])
  const { models } = useModels(active)

  const modelId =
    session.model?.trim() ||
    lastModel?.trim() ||
    selectedModelId?.trim() ||
    ""
  const modelInfo = models.find((m) => m.id === modelId)
  const maxTokens = modelInfo?.contextWindow ?? CONTEXT_BUDGET_TOKENS

  const totalInput = usage ? usage.input + (usage.cache_read ?? 0) : null
  const numCalls = Math.max(1, summary?.num_model_calls ?? 1)
  const contextUsed =
    totalInput === null ? null : Math.round(totalInput / numCalls)

  const sessionTokens = (totals?.input ?? 0) + (totals?.output ?? 0)
  const modelRows = Object.entries(modelUsage ?? {}).sort(
    (a, b) => b[1].input + b[1].output - (a[1].input + a[1].output),
  )
  const msgCount = modelRows.reduce((n, [, b]) => n + b.calls, 0)
  const cache = cacheTotalsFromModelUsage(modelUsage)
  const lastCacheRead = usage?.cache_read ?? 0
  const lastCacheWrite = usage?.cache_write ?? 0
  const readWrite =
    cache.cacheRead + cache.cacheWrite > 0
      ? `${formatTokens(cache.cacheRead)} / ${formatTokens(cache.cacheWrite)}`
      : usage
        ? `${formatTokens(lastCacheRead)} / ${formatTokens(lastCacheWrite)}`
        : "—"

  const stateLabel = streaming
    ? "running"
    : errorSeen > 0 && !usage
      ? "error"
      : "idle"

  const compactLabel =
    compaction &&
    (compaction.tokensBefore != null || compaction.tokensAfter != null)
      ? `${formatTokens(compaction.tokensBefore ?? 0)} → ${formatTokens(compaction.tokensAfter ?? 0)}`
      : compaction
        ? compaction.strategy || "done"
        : "—"

  const contextLabel =
    contextUsed != null
      ? `${formatTokens(contextUsed)} / ${formatTokens(maxTokens)}`
      : `— / ${formatTokens(maxTokens)}`

  return (
    <div
      className={cn(
        "flex h-full min-h-0 flex-col",
        !active && "pointer-events-none",
      )}
      aria-hidden={!active}
    >
      <div className="flex h-[var(--header-height)] shrink-0 items-center gap-1.5 border-b border-stroke-3 px-2.5">
        <Activity className="h-3.5 w-3.5 shrink-0 text-ink-faint" aria-hidden />
        <span className="min-w-0 flex-1 truncate text-sm text-ink">Status</span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2.5 py-3">
        <h2 className="mb-3 text-sm font-semibold text-ink">
          {sessionLabel(session)}
        </h2>

        <div className="mb-4 flex gap-3 border-b border-stroke-3 pb-3">
          <Metric
            label="Session"
            value={sessionTokens > 0 ? formatTokens(sessionTokens) : "—"}
          />
          <Metric label="Max" value={formatTokens(maxTokens)} />
          <Metric
            label="Msgs"
            value={msgCount > 0 ? String(msgCount) : "—"}
          />
        </div>

        <p className="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-ink-faint">
          Session
        </p>
        <div className="mb-4 flex flex-col">
          <DetailRow label="State" value={stateLabel} />
          <DetailRow label="Model" value={modelId || "—"} />
          <DetailRow label="Context" value={contextLabel} />
          <DetailRow label="Compact" value={compactLabel} />
          <DetailRow label="Read / write" value={readWrite} />
          <DetailRow
            label="Queue"
            value={`${queue.length} queued · ${streaming ? 1 : 0} running`}
          />
        </div>

        <p className="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-ink-faint">
          Models
        </p>
        {modelRows.length === 0 ? (
          <p className="text-sm text-ink-muted">No model usage yet</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {modelRows.map(([id, bucket]) => (
              <li
                key={id}
                className="rounded-md border border-stroke-3 px-2.5 py-1.5"
              >
                <p className="truncate text-sm text-ink" title={id}>
                  {id}
                </p>
                <p className="mt-0.5 text-xs text-ink-muted [font-variant-numeric:tabular-nums]">
                  {formatTokens(bucket.input)} in · {formatTokens(bucket.output)}{" "}
                  out · {bucket.calls} call{bucket.calls === 1 ? "" : "s"}
                </p>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}
