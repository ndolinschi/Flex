import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import App from "./App"
import { ErrorBoundary } from "./components/templates/ErrorBoundary"
import { TooltipProvider } from "./components/ui/tooltip"
import { registerBuiltinUiPlugins } from "./plugins/builtins"
import "./index.css"

registerBuiltinUiPlugins()

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <ErrorBoundary>
      <TooltipProvider delay={500}>
        <App />
      </TooltipProvider>
    </ErrorBoundary>
  </StrictMode>,
)
