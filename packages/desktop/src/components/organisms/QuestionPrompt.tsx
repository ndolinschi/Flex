import { useMemo, useState } from "react"
import { Button } from "@/components/ui/button"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Spinner } from "@/components/ui/spinner"

import { ErrorBanner } from "../molecules"
import { respondQuestion, toInvokeError } from "../../lib/tauri"
import type { PendingQuestion, Question } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"
import { log } from "../../lib/debug/log"
import { Input } from "@/components/ui/input"

type QuestionPromptProps = {
  question: PendingQuestion
}

type DraftAnswer = {
  selected: string[]
  custom: string
}

const emptyDraft = (): DraftAnswer => ({ selected: [], custom: "" })

/** Shared vertical rhythm for the docked quiz card (8px / 12px steps). */
const CARD_PAD = "px-3 pt-3 pb-3"
const SECTION_GAP = "mt-2.5"
const STACK_GAP = "gap-2"

const StepBody = ({
  q,
  draft,
  onChange,
  onOptionSelected,
}: {
  q: Question
  draft: DraftAnswer
  onChange: (next: DraftAnswer) => void
  /** Fired only when a single-select option is chosen — drives auto-advance. */
  onOptionSelected: () => void
}) => {
  const multi = !!q.multi_select
  const allowCustom = q.allow_custom !== false

  return (
    <fieldset className={cn("flex min-w-0 flex-col", STACK_GAP)}>
      <legend className="flex w-full min-w-0 flex-col gap-1">
        <span className="w-fit rounded-md bg-fill-3 px-1.5 py-0.5 text-xs font-medium tracking-[var(--tracking-caption)] text-ink-secondary">
          {q.header}
        </span>
        <span className="text-sm font-medium leading-snug text-ink">
          {q.question}
        </span>
      </legend>
      <div className={cn("flex flex-col", STACK_GAP)}>
        {multi ? (
          <ToggleGroup
            multiple
            value={draft.selected}
            onValueChange={(vals) =>
              onChange({ ...draft, selected: vals, custom: "" })
            }
            orientation="vertical"
            className="w-full flex-col items-stretch"
          >
            {q.options.map((opt) => (
              <ToggleGroupItem
                key={opt.label}
                value={opt.label}
                className={cn(
                  "h-auto w-full justify-start rounded-md border px-3 py-2 text-left text-sm leading-snug",
                  "border-stroke-3 bg-fill-5 text-ink-secondary",
                  "transition-colors duration-[var(--duration-fast)]",
                  "hover:border-stroke-2 hover:bg-fill-4 hover:text-ink-secondary",
                  "data-[pressed]:border-accent data-[pressed]:bg-accent-subtle data-[pressed]:text-ink",
                  "data-[pressed]:hover:bg-accent-subtle",
                )}
              >
                <span className="flex flex-col items-start gap-1">
                  <span className="font-medium leading-snug">{opt.label}</span>
                  {opt.description ? (
                    <span className="text-xs leading-snug text-ink-muted">
                      {opt.description}
                    </span>
                  ) : null}
                </span>
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        ) : (
          <RadioGroup
            value={draft.selected[0] ?? ""}
            onValueChange={(val) => {
              onChange({ selected: [val as string], custom: "" })
              onOptionSelected()
            }}
          >
            {q.options.map((opt) => {
              const active = draft.selected[0] === opt.label
              return (
                <label
                  key={opt.label}
                  className={cn(
                    "flex cursor-pointer items-start gap-2.5 rounded-md border px-3 py-2 text-sm leading-snug",
                    "transition-colors duration-[var(--duration-fast)]",
                    active
                      ? "border-accent bg-accent-subtle text-ink"
                      : "border-stroke-3 bg-fill-5 text-ink-secondary hover:border-stroke-2 hover:bg-fill-4",
                  )}
                >
                  <RadioGroupItem value={opt.label} className="mt-0.5 shrink-0" />
                  <span className="flex flex-col items-start gap-1">
                    <span className="font-medium leading-snug">{opt.label}</span>
                    {opt.description ? (
                      <span className="text-xs leading-snug text-ink-muted">
                        {opt.description}
                      </span>
                    ) : null}
                  </span>
                </label>
              )
            })}
          </RadioGroup>
        )}
        {allowCustom ? (
          <Input
            value={draft.custom}
            onChange={(e) => onChange({ selected: [], custom: e.target.value })}
            placeholder="Or type a custom answer…"
            aria-label={`Custom answer for ${q.header}`}
            className="h-8 w-full text-sm"
          />
        ) : null}
      </div>
    </fieldset>
  )
}

/** HITL modal for `AskUserQuestion` / `question_requested` events.
 *
 * Renders one question per step (reference-design wizard): a step
 * advances automatically on single-select choice, or via an explicit
 * Next button for multi-select / custom answers. Back preserves prior answers.
 * The last step's Next becomes Submit, which still builds the full
 * answers map client-side and sends it through the unchanged
 * `respondQuestion` wire call. */
export const QuestionPrompt = ({ question }: QuestionPromptProps) => {
  const setPendingQuestion = useAppStore((s) => s.setPendingQuestion)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [stepIndex, setStepIndex] = useState(0)
  const [stepKey, setStepKey] = useState(0)
  const [drafts, setDrafts] = useState<DraftAnswer[]>(() =>
    question.questions.map(() => emptyDraft()),
  )

  const total = question.questions.length
  const isLastStep = stepIndex === total - 1
  const currentQuestion = question.questions[stepIndex]
  const currentDraft = drafts[stepIndex]

  const currentStepFilled = useMemo(() => {
    const d = currentDraft
    return d.selected.length > 0 || d.custom.trim().length > 0
  }, [currentDraft])

  const showBack = stepIndex > 0
  // Single-select mid-steps auto-advance on option click — no hollow footer.
  // Custom text / multi-select / last step still need an explicit action.
  const showAdvance =
    !!currentQuestion.multi_select ||
    isLastStep ||
    currentDraft.custom.trim().length > 0
  const showFooter = showBack || showAdvance

  const goToStep = (next: number) => {
    setStepIndex(next)
    setStepKey((k) => k + 1)
    setError(null)
  }

  const handleBack = () => {
    if (stepIndex === 0) return
    goToStep(stepIndex - 1)
  }

  const handleAdvance = () => {
    if (!currentStepFilled) return
    if (isLastStep) {
      void handleSubmit()
      return
    }
    goToStep(stepIndex + 1)
  }

  const handleSubmit = async () => {
    setError(null)
    setIsSubmitting(true)
    try {
      const answers = question.questions.map((q, i) => {
        const d = drafts[i]
        const selected =
          d.custom.trim().length > 0 ? [d.custom.trim()] : d.selected
        return { question: q.question, selected }
      })
      await respondQuestion({
        sessionId: question.sessionId,
        requestId: question.requestId,
        answers,
      })
      setPendingQuestion(null)
    } catch (err) {
      const message = toInvokeError(err)
      log.error("session", "question respond failed", {
        requestId: question.requestId,
        sessionId: question.sessionId,
        error: message,
      })
      setError(message)
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <div
      role="dialog"
      aria-labelledby="question-title"
      // Same rail width as the composer pill below it (Composer docks this
      // as a sibling above the bubble) and no bottom margin — the two read
      // as one continuous stacked unit rather than a floating modal.
      className="w-full animate-modal-in"
      data-question-prompt
    >
      <div
        className={cn(
          // Match the composer bubble's surface/radius/shadow language, but
          // round only the top corners and drop the bottom shadow layer so
          // the seam where this card meets the composer reads as one solid
          // panel, not two stacked cards.
          "rounded-t-[var(--radius-composer)] border border-b-0 border-stroke-2",
          "bg-user-bubble shadow-[0_-4px_16px_-4px_var(--shadow-color)]",
          CARD_PAD,
        )}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 flex flex-col gap-0.5">
            <h3 id="question-title" className="text-sm font-semibold leading-snug text-ink">
              Agent needs your input
            </h3>
            <p className="text-xs leading-snug text-ink-muted">
              Answer to continue the turn.
            </p>
          </div>
          {total > 1 ? (
            <span className="shrink-0 pt-0.5 text-xs tabular-nums text-ink-faint">
              {stepIndex + 1} of {total}
            </span>
          ) : null}
        </div>

        <div key={stepKey} className={cn(SECTION_GAP, "animate-wizard-step-in")}>
          <StepBody
            q={currentQuestion}
            draft={currentDraft}
            onChange={(next) =>
              setDrafts((prev) =>
                prev.map((d, j) => (j === stepIndex ? next : d)),
              )
            }
            onOptionSelected={() => {
              if (currentQuestion.multi_select) return
              if (isLastStep) {
                void handleSubmit()
              } else {
                goToStep(stepIndex + 1)
              }
            }}
          />
        </div>

        {error ? (
          <div className={SECTION_GAP}>
            <ErrorBanner message={error} />
          </div>
        ) : null}

        {showFooter ? (
          <div
            className={cn(
              SECTION_GAP,
              "flex items-center gap-1.5",
              // Full action row (Back + Next/Submit): hairline separator.
              // Back-only mid-step: no border — avoids a hollow footer band
              // between the options and a lone Back control.
              showAdvance
                ? "justify-between border-t border-stroke-4 pt-2"
                : "justify-start",
            )}
          >
            {showBack ? (
              <Button size="sm" variant="ghost" onClick={handleBack}>
                Back
              </Button>
            ) : (
              <span />
            )}
            {showAdvance ? (
              <Button
                size="sm"
                disabled={!currentStepFilled || isSubmitting}
                onClick={handleAdvance}
              >
                {isSubmitting ? <Spinner data-icon="inline-start" /> : null}
                {isLastStep ? "Submit" : "Next"}
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  )
}
