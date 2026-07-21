type PromptHoverTipProps = {
  tip: {
    x: number
    y: number
    message: string
    fix?: string
  } | null
}

/** Fixed-position tooltip for an annotated prompt mark under the pointer. */
export const PromptHoverTip = ({ tip }: PromptHoverTipProps) => {
  if (!tip) return null

  return (
    <div
      className="pointer-events-none fixed z-[1100] max-w-xs -translate-x-1/2 -translate-y-full rounded-md bg-panel px-2.5 py-1.5 text-xs text-ink shadow-[var(--shadow-popover)]"
      style={{ left: tip.x, top: tip.y - 6 }}
      role="tooltip"
    >
      <p>{tip.message}</p>
      {tip.fix ? (
        <p className="mt-0.5 text-ink-muted">Click to apply: {tip.fix}</p>
      ) : null}
    </div>
  )
}
