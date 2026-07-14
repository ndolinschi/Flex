import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import App from "./App"
import { ErrorBoundary } from "./components/templates/ErrorBoundary"
import { registerBuiltinUiPlugins } from "./plugins/builtins"
import "./index.css"

registerBuiltinUiPlugins()

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
)
