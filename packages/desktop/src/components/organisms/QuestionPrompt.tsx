import { useMemo, useState } from "react"
import { Button, TextInput } from "../atoms"
import { ErrorBanner } from "../molecules"
import { respondQuestion, toInvokeError } from "../../lib/tauri"
import type { PendingQuestion, Question } from "../../lib/types"
import { useAppStore } from "../../stores/appStore"
import { cn } from "../../lib/utils"

type QuestionPromptProps = {
  question: PendingQuestion
}

type DraftAnswer = {
  selected: string[]
  custom: string
}

const emptyDraft = (): DraftAnswer => ({ selected: [], custom: "" })

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

  const handleToggle = (label: string) => {
    if (multi) {
      const has = draft.selected.includes(label)
      onChange({
        ...draft,
        selected: has
          ? draft.selected.filter((s) => s !== label)
          : [...draft.selected, label],
        custom: "",
      })
      return
    }
    onChange({ selected: [label], custom: "" })
    onOptionSelected()
  }

  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="text-sm font-medium leading-normal text-ink">
        <span className="mr-2 inline-block rounded bg-fill-3 px-1.5 py-0.5 text-xs text-ink-secondary">
          {q.header}
        </span>
        {q.question}
      </legend>
      <div className="flex flex-col gap-1.5">
        {q.options.map((opt) => {
          const active = draft.selected.includes(opt.label)
          return (
            <button
              key={opt.label}
              type="button"
              aria-pressed={active}
              onClick={() => handleToggle(opt.label)}
              className={cn(
                "rounded-md border px-3 py-2.5 text-left text-base",
                "transition-colors duration-[var(--duration-fast)]",
                active
                  ? "border-accent bg-accent-subtle text-ink"
                  : "border-stroke-3 bg-fill-5 text-ink-secondary hover:border-stroke-2 hover:bg-fill-4",
              )}
            >
              <span className="block font-medium">{opt.label}</span>
              {opt.description ? (
                <span className="mt-0.5 block text-sm text-ink-muted">
                  {opt.description}
                </span>
              ) : null}
            </button>
          )
        })}
      </div>
      {allowCustom ? (
        <TextInput
          value={draft.custom}
          onChange={(e) => onChange({ selected: [], custom: e.target.value })}
          placeholder="Or type a custom answer…"
          aria-label={`Custom answer for ${q.header}`}
          className="h-8 text-base"
        />
      ) : null}
    </fieldset>
  )
}

/** HITL modal for `AskUserQuestion` / `question_requested` events.
 *
 * Renders one question per step (reference-design wizard): a step
 * advances automatically on single-select choice, or via an explicit
 * Next button for multi-select steps. Back preserves prior answers.
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
      setError(toInvokeError(err))
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <div
      role="dialog"
      aria-labelledby="question-title"
      // Same rail width as the composer pill below it (see Composer.tsx's
      // slashRootRef container) and no bottom margin of its own — ChatShell
      // docks this flush above the composer so the two read as one
      // continuous stacked unit, reference-design style, rather
      // than a floating modal overlapping the feed.
      className="w-full max-w-[var(--content-rail)] animate-modal-in"
    >
      <div
        className={cn(
          // Match the composer bubble's surface/radius/shadow language, but
          // round only the top corners and drop the bottom shadow layer so
          // the seam where this card meets the composer reads as one solid
          // panel, not two stacked cards. bg-user-bubble is an opaque fill
          // (not the translucent panel token), so nothing behind it — feed
          // content included — can show through the header.
          "rounded-t-[var(--radius-composer)] border border-b-0 border-stroke-3",
          "bg-user-bubble px-4 pb-4 pt-3.5 shadow-[0_-4px_16px_-4px_var(--shadow-color)]",
        )}
      >
        <div className="flex items-start justify-between gap-3">
          <div>
            <h3 id="question-title" className="text-sm font-semibold text-ink">
              Agent needs your input
            </h3>
            <p className="mt-0.5 text-sm text-ink-muted">
              Answer to continue the turn.
            </p>
          </div>
          {total > 1 ? (
            <span className="shrink-0 pt-0.5 text-xs text-ink-faint">
              {stepIndex + 1} of {total}
            </span>
          ) : null}
        </div>

        <div key={stepKey} className="mt-4 animate-wizard-step-in">
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
          <div className="mt-3">
            <ErrorBanner message={error} />
          </div>
        ) : null}

        <div className="mt-4 flex items-center justify-between gap-1.5 border-t border-stroke-4 pt-3">
          <Button
            size="sm"
            variant="ghost"
            onClick={handleBack}
            className={cn(stepIndex === 0 && "invisible")}
          >
            Back
          </Button>

          {currentQuestion.multi_select || isLastStep ? (
            <Button
              size="sm"
              disabled={!currentStepFilled}
              isLoading={isSubmitting}
              onClick={handleAdvance}
            >
              {isLastStep ? "Submit" : "Next"}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}
