import { useAppStore } from "../../../stores/appStore"
import { useModels } from "../../../hooks/useModels"
import { cn, formatCost, formatTokens } from "../../../lib/utils"
import { Button } from "@/components/ui/button"
import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card"

/** Fallback context budget used for the usage ring when the selected
 * model's own context window isn't known (the reference design shows a similar %). */
const CONTEXT_BUDGET_TOKENS = 200_000

const ContextRing = ({ fraction }: { fraction: number }) => {
  const radius = 5
  const circumference = 2 * Math.PI * radius
  const clamped = Math.min(1, Math.max(0, fraction))

  return (
    <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden>
      <g transform="rotate(-90 7 7)">
        <circle
          cx="7"
          cy="7"
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeOpacity="0.28"
          strokeWidth="2"
        />
        <circle
          cx="7"
          cy="7"
          r={radius}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeDasharray={`${clamped * circumference} ${circumference}`}
        />
      </g>
    </svg>
  )
}

const UsageDetailRow = ({ label, value }: { label: string; value: string }) => (
  <div className="flex items-center justify-between gap-6">
    <span className="text-ink-muted">{label}</span>
    <span className="text-ink-secondary [font-variant-numeric:tabular-nums]">
      {value}
    </span>
  </div>
)

/** Context ring + % with a hover popover breaking down the last turn's usage. */
export const UsageRing = ({ sessionId }: { sessionId?: string | null }) => {
  const summary = useAppStore((s) =>
    sessionId ? s.lastTurnSummary[sessionId] : undefined,
  )
  const usage = useAppStore((s) =>
    sessionId ? s.lastTurnUsage[sessionId] : undefined,
  )
  const totals = useAppStore((s) =>
    sessionId ? s.sessionTotals[sessionId] : undefined,
  )
  const selectedModelId = useAppStore((s) => s.selectedModelId)
  const { models } = useModels(true)

  // `usage` (from `TurnSummary.usage`) is the SUM of every model call's
  // input tokens across the whole turn — an agent turn can make many calls,
  // each re-sending the growing conversation + cache-read tokens, so this
  // total is NOT how full the model's context window currently is (it can
  // read multiples of the window on a long tool-calling turn). What the
  // ring needs is the size of the single most recent request. The engine
  // doesn't yet expose a discrete "last request" token count to the
  // frontend (only the turn-aggregated `TokenUsage` — see `TurnSummary` in
  // wire.ts), so the best available approximation is this turn's average
  // per-call input: total input+cache_read divided by `num_model_calls`.
  // For a single-call turn (the common case) this equals the exact request
  // size; for multi-call turns it's a reasonable stand-in for current
  // context occupancy rather than the meaningless cumulative sum.
  const totalInput = usage ? usage.input + (usage.cache_read ?? 0) : null
  const numCalls = Math.max(1, summary?.num_model_calls ?? 1)
  const used = totalInput === null ? null : totalInput / numCalls
  if (used === null || !usage) return null
  const budget =
    models.find((m) => m.id === selectedModelId)?.contextWindow ??
    CONTEXT_BUDGET_TOKENS
  const rawFraction = used / budget
  // The ring itself must never exceed 100% — clamp the visual fraction, but
  // keep `rawFraction` around for the ">99%" over-limit label below.
  const fraction = Math.min(1, rawFraction)
  const isOverLimit = rawFraction > 1
  const nearLimitClass = isOverLimit
    ? "text-red"
    : fraction > 0.95
      ? "text-red"
      : fraction > 0.8
        ? "text-yellow"
        : "text-ink-muted"

  return (
    <HoverCard>
      <HoverCardTrigger
        render={
          <Button
            variant="ghost"
            className={cn(
              "h-6 gap-1 rounded-md px-1.5 text-sm font-normal hover:text-ink-secondary",
              nearLimitClass,
            )}
            aria-label="Context usage"
          />
        }
      >
        <ContextRing fraction={fraction} />
        <span className="[font-variant-numeric:tabular-nums]">
          {isOverLimit ? ">99%" : `${Math.round(fraction * 100)}%`}
        </span>
      </HoverCardTrigger>

      <HoverCardContent side="top" align="end" className="w-52 p-2.5 text-sm">
        <p className="mb-1.5 text-xs text-ink-faint">Last turn</p>
        <div className="flex flex-col gap-1">
          <UsageDetailRow label="Input" value={formatTokens(usage.input)} />
          <UsageDetailRow label="Output" value={formatTokens(usage.output)} />
          {usage.cache_read ? (
            <UsageDetailRow
              label="Cache read"
              value={formatTokens(usage.cache_read)}
            />
          ) : null}
          {usage.cache_write ? (
            <UsageDetailRow
              label="Cache write"
              value={formatTokens(usage.cache_write)}
            />
          ) : null}
          {usage.reasoning ? (
            <UsageDetailRow
              label="Reasoning"
              value={formatTokens(usage.reasoning)}
            />
          ) : null}
          <UsageDetailRow label="Budget" value={formatTokens(budget)} />
          {summary && typeof summary.cost_usd === "number" ? (
            <>
              <div className="my-0.5 border-t border-stroke-3" />
              <UsageDetailRow label="Cost" value={formatCost(summary.cost_usd)} />
            </>
          ) : null}
          {totals ? (
            <>
              <div className="my-0.5 border-t border-stroke-3" />
              <p className="text-xs text-ink-faint">Session total</p>
              <UsageDetailRow
                label="Tokens"
                value={formatTokens(totals.input + totals.output)}
              />
              {totals.costUsd > 0 ? (
                <UsageDetailRow label="Cost" value={formatCost(totals.costUsd)} />
              ) : null}
            </>
          ) : null}
        </div>
      </HoverCardContent>
    </HoverCard>
  )
}
