import { Component, type ErrorInfo, type ReactNode } from "react"

type ErrorBoundaryProps = {
  children: ReactNode
}

type ErrorBoundaryState = {
  error: Error | null
}

/** Root-level render-error boundary. Without this, one uncaught render error
 * anywhere in the tree unmounts React entirely and leaves a white screen —
 * this catches it, logs the component stack, and renders a calm fallback
 * instead. Mounted in `main.tsx` around `<App/>`, inside `<StrictMode>`.
 * Dependency-free by design (no atoms/molecules imports) so it can never
 * itself fail to render because of an app-level regression. Uses the same
 * CSS custom properties the rest of the app themes off of (`--color-*`,
 * set on `<html data-theme>` in `stores/appStore.ts`), so it stays
 * dark/light aware even though it renders above the themed component tree. */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[ErrorBoundary] uncaught render error:", error, info.componentStack)
  }

  render(): ReactNode {
    const { error } = this.state
    if (!error) return this.props.children

    return (
      <div
        style={{
          position: "fixed",
          inset: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: 24,
          background: "var(--color-bg, #1a1a1a)",
          color: "var(--color-ink, #e6e6e6)",
        }}
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 12,
            maxWidth: 480,
            width: "100%",
            padding: "20px 24px",
            borderRadius: 12,
            background: "var(--color-panel, #242424)",
            border: "1px solid var(--color-border, rgba(128,128,128,0.3))",
          }}
        >
          <p style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>Something went wrong</p>
          <p
            style={{
              margin: 0,
              fontSize: 12,
              fontFamily:
                "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
              color: "var(--color-ink-muted, #999999)",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
          >
            {error.message || String(error)}
          </p>
          <button
            type="button"
            onClick={() => location.reload()}
            style={{
              alignSelf: "flex-start",
              marginTop: 8,
              padding: "6px 14px",
              fontSize: 13,
              borderRadius: 6,
              border: "1px solid var(--color-border-strong, rgba(128,128,128,0.5))",
              background: "var(--color-fill-3, rgba(128,128,128,0.15))",
              color: "var(--color-ink, #e6e6e6)",
              cursor: "pointer",
            }}
          >
            Reload
          </button>
        </div>
      </div>
    )
  }
}
