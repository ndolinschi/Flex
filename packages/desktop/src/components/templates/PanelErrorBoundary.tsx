import { Component, type ErrorInfo, type ReactNode } from "react"
import { log } from "../../lib/debug/log"
import { ToolQueryError } from "../molecules"

type PanelErrorBoundaryProps = {
  children: ReactNode
  /** Shown in the recovery title, e.g. "Changes", "Terminal". */
  label?: string
}

type PanelErrorBoundaryState = {
  error: Error | null
}

/**
 * Scoped error boundary for tool / file panels.
 * Isolates render crashes so one tab does not take down the content pane
 * (app-level ErrorBoundary still catches shell failures).
 */
export class PanelErrorBoundary extends Component<
  PanelErrorBoundaryProps,
  PanelErrorBoundaryState
> {
  state: PanelErrorBoundaryState = { error: null }

  static getDerivedStateFromError(error: Error): PanelErrorBoundaryState {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    const label = this.props.label ?? "panel"
    console.error(`[PanelErrorBoundary:${label}]`, error, info.componentStack)
    try {
      log.error("ui", "tool panel render error", {
        label,
        message: error.message,
        stack: error.stack,
        componentStack: info.componentStack,
      })
    } catch {
      // logging must never throw into the boundary
    }
  }

  private handleRetry = () => {
    this.setState({ error: null })
  }

  render(): ReactNode {
    const { error } = this.state
    if (!error) return this.props.children

    const label = this.props.label?.trim()
    return (
      <ToolQueryError
        title={label ? `${label} crashed` : "Panel crashed"}
        error={error}
        fallbackMessage="An unexpected error occurred while rendering this panel."
        onRetry={this.handleRetry}
      />
    )
  }
}
