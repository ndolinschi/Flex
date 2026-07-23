import { useEffect, useRef, useState } from "react"
import { Button } from "@/components/ui/button"
import { ErrorBanner } from "./ErrorBanner"
import { isBrowserPreview } from "../../lib/browserPreview"
import type { CopilotAuthStart } from "../../lib/types"
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

type CopilotSignInDialogProps = {
  open: boolean
  onClose: () => void
  onSuccess: () => void
  start: () => Promise<CopilotAuthStart>
  wait: (sessionId: string) => Promise<{ signedIn: boolean }>
  cancel: (sessionId: string) => Promise<void>
}

export const CopilotSignInDialog = ({
  open,
  onClose,
  onSuccess,
  start,
  wait,
  cancel,
}: CopilotSignInDialogProps) => {
  const sessionIdRef = useRef<string | null>(null)
  const startRef = useRef(start)
  const waitRef = useRef(wait)
  const cancelRef = useRef(cancel)
  const onSuccessRef = useRef(onSuccess)
  const onCloseRef = useRef(onClose)
  const [auth, setAuth] = useState<CopilotAuthStart | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [phase, setPhase] = useState<"starting" | "waiting" | "done">("starting")
  const [copied, setCopied] = useState(false)

  startRef.current = start
  waitRef.current = wait
  cancelRef.current = cancel
  onSuccessRef.current = onSuccess
  onCloseRef.current = onClose

  useEffect(() => {
    if (!open) {
      setAuth(null)
      setError(null)
      setPhase("starting")
      setCopied(false)
      sessionIdRef.current = null
      return
    }

    let cancelled = false
    const run = async () => {
      setPhase("starting")
      setError(null)
      try {
        const started = await startRef.current()
        if (cancelled) {
          await cancelRef.current(started.sessionId).catch(() => undefined)
          return
        }
        sessionIdRef.current = started.sessionId
        setAuth(started)
        setPhase("waiting")
        await waitRef.current(started.sessionId)
        if (cancelled) return
        sessionIdRef.current = null
        setPhase("done")
        onSuccessRef.current()
      } catch (err) {
        if (cancelled) return
        sessionIdRef.current = null
        setError(err instanceof Error ? err.message : String(err))
        setPhase("starting")
      }
    }
    void run()

    return () => {
      cancelled = true
      const id = sessionIdRef.current
      sessionIdRef.current = null
      if (id) {
        void cancelRef.current(id).catch(() => undefined)
      }
    }
  }, [open])

  const handleClose = async () => {
    const id = sessionIdRef.current
    sessionIdRef.current = null
    if (id) {
      await cancelRef.current(id).catch(() => undefined)
    }
    onCloseRef.current()
  }

  const handleCopy = async () => {
    if (!auth?.userCode) return
    try {
      await navigator.clipboard.writeText(auth.userCode)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      setError("Could not copy code to clipboard")
    }
  }

  const handleOpenGithub = async () => {
    if (!auth?.verificationUri) return
    try {
      if (isBrowserPreview()) {
        window.open(auth.verificationUri, "_blank", "noopener,noreferrer")
        return
      }
      const { openUrl } = await import("@tauri-apps/plugin-opener")
      await openUrl(auth.verificationUri)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleTryAgain = () => {
    setError(null)
    setAuth(null)
    setPhase("starting")
    void (async () => {
      try {
        const started = await startRef.current()
        sessionIdRef.current = started.sessionId
        setAuth(started)
        setPhase("waiting")
        await waitRef.current(started.sessionId)
        sessionIdRef.current = null
        setPhase("done")
        onSuccessRef.current()
      } catch (err) {
        sessionIdRef.current = null
        setError(err instanceof Error ? err.message : String(err))
        setPhase("starting")
      }
    })()
  }

  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) void handleClose()
      }}
    >
      <AlertDialogContent
        size="sm"
        className="max-w-[min(100%,28rem)] sm:max-w-md"
      >
        <AlertDialogHeader>
          <AlertDialogTitle>Sign in to GitHub Copilot</AlertDialogTitle>
          <AlertDialogDescription>
            Enter this code on GitHub, then return here. Waiting stops when you
            approve the sign-in.
          </AlertDialogDescription>
        </AlertDialogHeader>

        {error ? (
          <ErrorBanner message={error} onDismiss={() => setError(null)} />
        ) : null}

        {phase === "starting" && !error ? (
          <p className="text-sm text-ink-muted">Requesting a device code…</p>
        ) : null}

        {auth ? (
          <div className="flex flex-col gap-3">
            <div className="rounded-md border border-stroke-3 bg-elevated px-3 py-3 text-center">
              <p className="text-xs font-medium uppercase tracking-widest text-ink-faint">
                User code
              </p>
              <p className="mt-1 font-mono text-2xl font-semibold tracking-widest text-ink">
                {auth.userCode}
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" size="sm" onClick={() => void handleCopy()}>
                {copied ? "Copied" : "Copy code"}
              </Button>
              <Button size="sm" onClick={() => void handleOpenGithub()}>
                Open GitHub
              </Button>
            </div>
            {phase === "waiting" ? (
              <p className="text-sm text-ink-muted" role="status">
                Waiting for confirmation on GitHub…
              </p>
            ) : null}
          </div>
        ) : null}

        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          {error ? (
            <AlertDialogAction
              onClick={(e) => {
                e.preventDefault()
                handleTryAgain()
              }}
            >
              Try again
            </AlertDialogAction>
          ) : null}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
