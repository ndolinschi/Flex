import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

type PromptQuestionsFormProps = {
  questions: string[]
  answers: Record<string, string>
  busy: boolean
  onAnswerChange: (question: string, value: string) => void
  onSubmit: () => void
}

/** Coach clarifying questions after a Verify pass — answer & re-verify. */
export const PromptQuestionsForm = ({
  questions,
  answers,
  busy,
  onAnswerChange,
  onSubmit,
}: PromptQuestionsFormProps) => {
  if (questions.length === 0) return null

  return (
    <div className="shrink-0 border-t border-stroke-3 px-2.5 py-2">
      <p className="mb-1.5 text-xs font-medium text-ink-secondary">
        Coach questions
      </p>
      <ul className="flex flex-col gap-2">
        {questions.map((q) => (
          <li key={q} className="flex flex-col gap-1">
            <label className="text-xs text-ink">{q}</label>
            <Input
              type="text"
              value={answers[q] ?? ""}
              onChange={(e) => onAnswerChange(q, e.target.value)}
              className="h-7 rounded-md border-stroke-3 bg-fill-5 px-2 text-xs text-ink focus-visible:ring-1 focus-visible:ring-stroke-2"
              placeholder="Your answer…"
            />
          </li>
        ))}
      </ul>
      <Button
        variant="link"
        disabled={busy}
        onClick={onSubmit}
        className="mt-2 h-auto px-0 py-0 text-xs text-accent font-normal"
      >
        Answer & re-verify
      </Button>
    </div>
  )
}
