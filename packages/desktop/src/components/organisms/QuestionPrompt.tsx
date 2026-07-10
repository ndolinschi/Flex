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

const QuestionBlock = ({
  q,
  draft,
  onChange,
}: {
  q: Question
  draft: DraftAnswer
  onChange: (next: DraftAnswer) => void
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
  }

  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="text-sm font-medium text-ink">
        <span className="mr-1.5 rounded bg-fill-3 px-1.5 py-0.5 text-xs text-ink-secondary">
          {q.header}
        </span>
        {q.question}
      </legend>
      <div className="flex flex-col gap-1">
        {q.options.map((opt) => {
          const active = draft.selected.includes(opt.label)
          return (
            <button
              key={opt.label}
              type="button"
              aria-pressed={active}
              onClick={() => handleToggle(opt.label)}
              className={cn(
                "rounded-md border px-2.5 py-2 text-left text-base",
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
          onChange={(e) =>
            onChange({ selected: [], custom: e.target.value })
          }
          placeholder="Or type a custom answer…"
          aria-label={`Custom answer for ${q.header}`}
          className="h-8 text-base"
        />
      ) : null}
    </fieldset>
  )
}

/** HITL modal for `AskUserQuestion` / `question_requested` events. */
export const QuestionPrompt = ({ question }: QuestionPromptProps) => {
  const setPendingQuestion = useAppStore((s) => s.setPendingQuestion)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [drafts, setDrafts] = useState<DraftAnswer[]>(() =>
    question.questions.map(() => emptyDraft()),
  )

  const canSubmit = useMemo(() => {
    return question.questions.every((_, i) => {
      const d = drafts[i]
      return d.selected.length > 0 || d.custom.trim().length > 0
    })
  }, [drafts, question.questions])

  const handleSubmit = async () => {
    if (!canSubmit) return
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
      className="w-full max-w-lg animate-tray-in"
    >
      <div className="rounded-xl bg-panel p-3 shadow-lg">
        <h3 id="question-title" className="text-sm font-semibold text-ink">
          Agent needs your input
        </h3>
        <p className="mt-0.5 text-sm text-ink-muted">
          Answer to continue the turn.
        </p>

        <div className="mt-3 flex flex-col gap-4">
          {question.questions.map((q, i) => (
            <QuestionBlock
              key={`${q.header}-${i}`}
              q={q}
              draft={drafts[i]}
              onChange={(next) =>
                setDrafts((prev) => prev.map((d, j) => (j === i ? next : d)))
              }
            />
          ))}
        </div>

        {error ? (
          <div className="mt-2">
            <ErrorBanner message={error} />
          </div>
        ) : null}

        <div className="mt-3 flex justify-end gap-1.5">
          <Button
            size="sm"
            disabled={!canSubmit}
            isLoading={isSubmitting}
            onClick={() => void handleSubmit()}
          >
            Submit
          </Button>
        </div>
      </div>
    </div>
  )
}
