import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import App from "./App"
import { ErrorBoundary } from "./components/templates/ErrorBoundary"
import { TooltipProvider } from "./components/ui/tooltip"
import { detectWindowHost } from "./lib/windowChrome"
import { registerBuiltinUiPlugins } from "./plugins/builtins"
import "./index.css"

document.documentElement.setAttribute("data-platform", detectWindowHost())

registerBuiltinUiPlugins()

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <ErrorBoundary>
      <TooltipProvider delay={300}>
        <App />
      </TooltipProvider>
    </ErrorBoundary>
  </StrictMode>,
)
