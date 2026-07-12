import { defineConfig } from "vitest/config"
import react from "@vitejs/plugin-react"

// Minimal test config, separate from vite.config.ts (which is tauri-dev
// specific: fixed port/host, HMR-over-websocket, `src-tauui` watch-ignore —
// none of that applies to a one-shot node-environment test run). Only the
// react plugin is carried over, so `.tsx` files reached transitively (e.g.
// via the `components/molecules` barrel) still transform; the Tailwind vite
// plugin is deliberately omitted since these are pure data-function tests
// with no CSS/DOM involved.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "node",
    // Playwright specs live under e2e/ and must not be collected by Vitest.
    exclude: ["**/node_modules/**", "**/dist/**", "**/e2e/**", "**/.soak/**"],
  },
})
