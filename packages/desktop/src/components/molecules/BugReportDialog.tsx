import { useEffect, useState } from "react"
import { openUrl } from "@tauri-apps/plugin-opener"
import { Button, TextArea } from "../atoms"
import {
  BUG_REPORT_PRIVACY_URL,
  BUG_REPORT_TERMS_URL,
  buildBugReportUrl,
  type BugReportContext,
} from "../../lib/bugReport"
import { appVersion, toInvokeError } from "../../lib/tauri"
import { useAppStore } from "../../stores/appStore"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"

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
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onClose()
      }}
    >
      <DialogContent
        showCloseButton
        data-suppress-native-webview=""
        className={cn(
          "top-[12vh] max-w-[440px] translate-y-0 gap-0 rounded-2xl p-0 sm:max-w-[440px]",
        )}
        onEscapeKeyDown={(e) => {
          if (busy) e.preventDefault()
        }}
        onPointerDownOutside={(e) => {
          if (busy) e.preventDefault()
        }}
      >
        <DialogHeader className="gap-1 px-5 pt-5 pb-1 text-left">
          <DialogTitle className="text-[18px] font-medium tracking-tight text-ink">
            Submit Bug
          </DialogTitle>
          <DialogDescription className="sr-only">
            Submit a bug report with session context
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3 px-5 pb-5 pt-2">
          <div className="rounded-lg bg-fill-4/80 px-3.5 py-3 text-base leading-relaxed text-ink-secondary">
            <p className="text-ink">
              Submitting this feedback report will send the following
              information to the maintainers:
            </p>
            <ul className="mt-2 list-disc space-y-1 pl-4">
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
            <TextArea
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
            <p className="text-xs text-danger" role="alert">
              {error}
            </p>
          ) : null}

          <DialogFooter className="mx-0 mb-0 border-0 bg-transparent p-0 pt-1 sm:justify-end">
            <Button
              size="sm"
              variant="secondary"
              disabled={busy}
              onClick={onClose}
            >
              Cancel
            </Button>
            <Button
              size="sm"
              variant="primary"
              isLoading={busy}
              disabled={!canSubmit}
              onClick={() => void handleSubmit()}
            >
              Submit
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  )
}
