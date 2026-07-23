import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react"
import { ErrorBanner } from "../../molecules"
import { Textarea } from "@/components/ui/textarea"
import { AtMentionTray } from "../composer/AtMentionTray"
import { SlashCommandTray } from "../composer/SlashCommandTray"
import { useComposerAutocomplete } from "../../../hooks/useComposerAutocomplete"
import { useInlineCompletion } from "../../../hooks/useInlineCompletion"
import { useSessions } from "../../../hooks/useSessions"
import {
  annotationsFromFindings,
  estimateTokens,
  segmentAnnotatedPrompt,
  type PromptAnnotation,
} from "../../../lib/promptEngineering"
import {
  reviewPrompt,
  toInvokeError,
  type PromptReview,
  type PromptReviewAnswer,
} from "../../../lib/tauri"
import type { SessionId } from "../../../lib/types"
import { cn } from "../../../lib/utils"
import { useAppStore } from "../../../stores/appStore"
import { CompletionSetupModal } from "../../../plugins/prompt-completion"
import {
  PromptFindingsList,
  PromptMarksPanel,
} from "./PromptMarksPanel"
import { PromptQuestionsForm } from "./PromptQuestionsForm"
import { PromptTabHeader } from "./PromptTabHeader"

type PromptTabProps = {
  sessionId: SessionId
  active: boolean
}

export const PromptTab = ({ sessionId, active }: PromptTabProps) => {
  const draft = useAppStore((s) => s.draftsBySession[sessionId] ?? "")
  const setComposerDraft = useAppStore((s) => s.setComposerDraft)
  const attachments = useAppStore((s) => s.attachments)
  const addAttachment = useAppStore((s) => s.addAttachment)
  const { sessions } = useSessions()
  const cwd = sessions.find((s) => s.id === sessionId)?.cwd

  const [review, setReview] = useState<PromptReview | null>(null)
  const [showMarks, setShowMarks] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [insertOpen, setInsertOpen] = useState(false)
  const [answers, setAnswers] = useState<Record<string, string>>({})

  const rootRef = useRef<HTMLDivElement>(null)
  const textareaRef = useRef<HTMLTextAreaElement | null>(null)
  const promptBackdropRef = useRef<HTMLDivElement>(null)

  const setDraft = (value: string) => {
    setComposerDraft(value, sessionId)
  }

  const {
    caret,
    setCaret,
    slashOpen,
    slashMatches,
    slashHighlight,
    setSlashHighlight,
    atOpen,
    fileHits,
    atHighlight,
    setAtHighlight,
    setAtDismissed,
    handleInsertCommand,
    handleInsertFile,
  } = useComposerAutocomplete({
    composerDraft: draft,
    setComposerDraft: setDraft,
    attachments,
    addAttachment,
    cwd,
    textareaRef,
    enabled: active,
    slashAtCaret: true,
  })

  const {
    suggestion,
    accept: acceptCompletion,
    dismiss: dismissCompletion,
    setupOpen,
    setSetupOpen,
    dismissSetup,
  } = useInlineCompletion({
    draft,
    caret,
    traysOpen: atOpen || slashOpen || showMarks,
    surfaceEnabled: active,
    setDraft,
    setCaret,
    focusCaret: (nextCaret) => {
      window.requestAnimationFrame(() => {
        const ta = textareaRef.current
        if (!ta) return
        ta.focus()
        ta.setSelectionRange(nextCaret, nextCaret)
      })
    },
  })

  const liveFindings = useMemo(() => {
    if (!review) return []
    return review.findings.filter((f) => f.quote && draft.includes(f.quote))
  }, [review, draft])

  const annotations = useMemo(
    () => annotationsFromFindings(draft, liveFindings),
    [draft, liveFindings],
  )
  const segments = useMemo(
    () => segmentAnnotatedPrompt(draft, annotations),
    [draft, annotations],
  )
  const tokens = estimateTokens(draft)
  const chars = draft.length
  const questions = review?.questions ?? []

  const runVerify = async (withAnswers?: PromptReviewAnswer[]) => {
    if (!draft.trim() || busy) return
    setBusy(true)
    setError(null)
    try {
      const result = await reviewPrompt(
        sessionId,
        draft,
        withAnswers?.length ? withAnswers : undefined,
      )
      setReview(result)
      setShowMarks((result.findings?.length ?? 0) > 0)
      const nextAnswers: Record<string, string> = {}
      for (const q of result.questions ?? []) nextAnswers[q] = answers[q] ?? ""
      setAnswers(nextAnswers)
    } catch (err) {
      setError(toInvokeError(err))
    } finally {
      setBusy(false)
    }
  }

  const applyFix = (ann: PromptAnnotation) => {
    if (!ann.fix || !review) return
    const next = draft.slice(0, ann.start) + ann.fix + draft.slice(ann.end)
    setDraft(next)
    setReview({
      ...review,
      findings: review.findings.filter(
        (f) => !(f.quote === ann.quote && f.message === ann.message),
      ),
    })
  }

  const dismissFinding = (ann: PromptAnnotation) => {
    if (!review) return
    setReview({
      ...review,
      findings: review.findings.filter(
        (f) => !(f.quote === ann.quote && f.message === ann.message),
      ),
    })
  }

  const submitAnswers = () => {
    const payload: PromptReviewAnswer[] = questions.map((q) => ({
      question: q,
      answer: answers[q] ?? "",
    }))
    void runVerify(payload)
  }

  const sendFromEditor = () => {
    window.dispatchEvent(new Event("flex:focus-composer"))
    window.requestAnimationFrame(() => {
      const ta = document.querySelector<HTMLTextAreaElement>("[data-composer]")
      if (!ta) return
      ta.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Enter",
          metaKey: true,
          bubbles: true,
        }),
      )
    })
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (atOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setAtHighlight((i) => Math.min(i + 1, fileHits.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setAtHighlight((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.metaKey && !e.ctrlKey)) {
        e.preventDefault()
        const pick = fileHits[atHighlight] ?? fileHits[0]
        if (pick) handleInsertFile(pick)
        return
      }
      if (e.key === "Escape") {
        e.preventDefault()
        setAtDismissed(true)
        return
      }
    }
    if (slashOpen) {
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setSlashHighlight((i) => Math.min(i + 1, slashMatches.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setSlashHighlight((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.metaKey && !e.ctrlKey)) {
        e.preventDefault()
        const pick = slashMatches[slashHighlight] ?? slashMatches[0]
        if (pick) handleInsertCommand(pick.name)
        return
      }
      if (e.key === "Escape") {
        e.preventDefault()
        return
      }
    }
    if (suggestion && !atOpen && !slashOpen) {
      if (e.key === "Tab") {
        e.preventDefault()
        acceptCompletion()
        return
      }
      if (e.key === "Escape") {
        e.preventDefault()
        dismissCompletion()
        return
      }
    }
  }

  useEffect(() => {
    if (annotations.length === 0) setShowMarks(false)
  }, [annotations.length])

  return (
    <div
      ref={rootRef}
      className="relative flex h-full min-h-0 flex-col"
      aria-hidden={!active}
    >
      <SlashCommandTray
        open={slashOpen && !showMarks}
        anchorRef={rootRef}
        matches={slashMatches}
        highlight={slashHighlight}
        onSelect={handleInsertCommand}
      />
      <AtMentionTray
        open={atOpen && !showMarks}
        anchorRef={rootRef}
        hits={fileHits}
        highlight={atHighlight}
        onClose={() => setAtDismissed(true)}
        onSelect={handleInsertFile}
      />

      <PromptTabHeader
        chars={chars}
        tokens={tokens}
        draft={draft}
        setDraft={setDraft}
        insertOpen={insertOpen}
        setInsertOpen={setInsertOpen}
        annotationsCount={annotations.length}
        showMarks={showMarks}
        setShowMarks={setShowMarks}
        busy={busy}
        onVerify={() => void runVerify()}
        onSend={sendFromEditor}
      />

      {error ? (
        <ErrorBanner
          message={error}
          className="shrink-0 rounded-none border-x-0 border-t-0 px-2.5 py-1.5 text-xs"
        />
      ) : null}

      {review?.summary ? (
        <p className="shrink-0 border-b border-stroke-3 px-2.5 py-1.5 text-xs text-ink-secondary">
          {review.summary}
        </p>
      ) : null}

      <div className="relative min-h-0 flex-1">
        {showMarks && annotations.length > 0 ? (
          <PromptMarksPanel
            segments={segments}
            annotations={annotations}
            onApplyFix={applyFix}
          />
        ) : (
          <div className="relative h-full min-h-0">
            <div
              ref={promptBackdropRef}
              aria-hidden
              className={cn(
                "pointer-events-none absolute inset-0 overflow-hidden",
                "whitespace-pre-wrap break-words px-2.5 py-2 text-sm leading-relaxed text-ink",
              )}
            >
              {draft}
              {suggestion ? (
                <span className="text-ink-faint">{suggestion}</span>
              ) : null}
              {"​"}
            </div>
            <Textarea
              ref={textareaRef}
              value={draft}
              onChange={(e) => {
                setDraft(e.target.value)
                setCaret(e.target.selectionStart ?? e.target.value.length)
              }}
              onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
              onKeyDown={handleKeyDown}
              onScroll={(e) => {
                const bd = promptBackdropRef.current
                if (bd) bd.scrollTop = e.currentTarget.scrollTop
              }}
              placeholder="Write the prompt… Use @ for files/MCP and / for commands. Then Verify."
              className={cn(
                "relative h-full min-h-0 w-full field-sizing-fixed resize-none overflow-y-auto",
                "rounded-none border-0 bg-transparent px-2.5 py-2 shadow-none",
                "text-sm leading-relaxed text-transparent caret-ink outline-none",
                "placeholder:text-ink-muted",
                "focus-visible:border-0 focus-visible:ring-0",
              )}
              aria-label="Prompt draft"
            />
          </div>
        )}
      </div>

      <PromptQuestionsForm
        questions={questions}
        answers={answers}
        busy={busy}
        onAnswerChange={(q, value) =>
          setAnswers((prev) => ({ ...prev, [q]: value }))
        }
        onSubmit={submitAnswers}
      />

      <PromptFindingsList
        annotations={annotations}
        onApplyFix={applyFix}
        onDismissFinding={dismissFinding}
        hasReview={!!review}
        hasQuestions={questions.length > 0}
      />

      <CompletionSetupModal
        open={setupOpen}
        onClose={() => setSetupOpen(false)}
        onDismiss={() => void dismissSetup()}
      />
    </div>
  )
}
