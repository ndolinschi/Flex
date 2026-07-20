import { useEffect, useState } from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import { Textarea } from "@/components/ui/textarea"
import { ErrorBanner } from "./ErrorBanner"
import {
  BUG_REPORT_PRIVACY_URL,
  BUG_REPORT_TERMS_URL,
  buildBugReportUrl,
  type BugReportContext,
} from "../../lib/bugReport"
import { appVersion, toInvokeError } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import { Spinner } from "@/components/ui/spinner"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

type BugReportDialogProps = {
  open: boolean
  onClose: () => void
}

const collectTaskIds = (): string[] => {
  const state = useAppStore.getState()
  const ids = new Set<string>()
  if (state.activeSessionId) ids.add(state.activeSessionId)
  for (const id of Object.keys(state.streamingSessions)) {
    if (state.streamingSessions[id]) ids.add(id)
  }
  for (const id of Object.keys(state.completedTurns)) {
    ids.add(id)
    const turn = state.completedTurns[id]
    if (turn && turn !== "__ended__") ids.add(turn)
  }
  return [...ids].slice(0, 12)
}

/** Google-style Submit Bug modal: disclosure of what is sent + free-text note. */
export const BugReportDialog = ({ open, onClose }: BugReportDialogProps) => {
  const [note, setNote] = useState("")
  const [version, setVersion] = useState("…")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const pushToast = useAppStore((s) => s.pushToast)
  const sessionId = useAppStore((s) => s.activeSessionId)

  useEffect(() => {
    if (!open) return
    setNote("")
    setError(null)
    setBusy(false)
    void appVersion()
      .then(setVersion)
      .catch(() => setVersion("unknown"))
  }, [open])

  const canSubmit = note.trim().length > 0 && !busy

  const handleSubmit = async () => {
    if (!canSubmit) return
    setBusy(true)
    setError(null)
    const ctx: BugReportContext = {
      appVersion: version === "…" ? "unknown" : version,
      os: typeof navigator !== "undefined" ? navigator.platform : "unknown",
      sessionId,
      taskIds: collectTaskIds(),
    }
    const url = buildBugReportUrl(note, ctx)
    try {
      await openUrl(url)
      pushToast("Opened bug report form", "success")
      onClose()
    } catch (err) {
      setError(toInvokeError(err))
      setBusy(false)
    }
  }

  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onClose()
      }}
    >
      <AlertDialogContent
        size="sm"
        className="max-w-[min(100%,28rem)] sm:max-w-md"
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Submit Bug</AlertDialogTitle>
          <AlertDialogDescription>
            Submitting this report sends your app ID and task IDs from this
            session to the maintainers. Do not include personal or sensitive
            information.
          </AlertDialogDescription>
        </AlertDialogHeader>

        <div className="flex flex-col gap-3">
          <div className="rounded-lg bg-fill-4/80 px-3.5 py-3 text-base leading-relaxed text-ink-secondary">
            <p className="text-ink">
              Submitting this feedback report will send the following
              information to the maintainers:
            </p>
            <ul className="mt-2 flex flex-col gap-1 list-disc pl-4">
              <li>The ID of your app</li>
              <li>The IDs of tasks you executed in this session</li>
            </ul>
            <p className="mt-2.5 text-sm text-ink-muted">
              Your response is Feedback under the{" "}
              <a
                href={BUG_REPORT_TERMS_URL}
                target="_blank"
                rel="noreferrer"
                className="text-accent underline-offset-2 hover:underline"
                onClick={(e) => {
                  e.preventDefault()
                  void openUrl(BUG_REPORT_TERMS_URL)
                }}
              >
                Terms
              </a>
              , and may be used to improve our services subject to our{" "}
              <a
                href={BUG_REPORT_PRIVACY_URL}
                target="_blank"
                rel="noreferrer"
                className="text-accent underline-offset-2 hover:underline"
                onClick={(e) => {
                  e.preventDefault()
                  void openUrl(BUG_REPORT_PRIVACY_URL)
                }}
              >
                Privacy Policy
              </a>
              . Do not submit personal, sensitive, or confidential information.
            </p>
          </div>

          <label className="flex flex-col gap-1.5">
            <span className="text-base font-medium text-ink">
              Tell us what went wrong
            </span>
            <Textarea
              value={note}
              onChange={(e) => setNote(e.target.value)}
              rows={5}
              placeholder="What happened? What did you expect?"
              aria-label="Tell us what went wrong"
              className="min-h-[7.5rem] resize-y bg-bg"
              disabled={busy}
            />
          </label>

          {error ? (
            <ErrorBanner message={error} onDismiss={() => setError(null)} />
          ) : null}
        </div>

        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            disabled={!canSubmit}
            onClick={(e) => {
              e.preventDefault()
              if (!canSubmit) return
              void handleSubmit()
            }}
          >
            {busy ? <Spinner data-icon="inline-start" /> : null}
            Submit
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
