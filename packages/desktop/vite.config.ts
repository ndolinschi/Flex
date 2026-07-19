import path from "node:path"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST

const rootDir = path.dirname(fileURLToPath(import.meta.url))

// React Compiler — enabled carefully after Waves 1–2. If tsc/vitest fail
// badly with it on, leave disabled and note in COMPONENTS.md.
const enableReactCompiler = true

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    react(
      enableReactCompiler
        ? {
            babel: {
              plugins: [["babel-plugin-react-compiler", { target: "19" }]],
            },
          }
        : undefined,
    ),
    tailwindcss(),
  ],

  resolve: {
    alias: {
      "@": path.resolve(rootDir, "./src"),
    },
  },

  build: {
    // Monaco alone is ~3.9MB minified — expected for a full editor (+ workers).
    // Other vendor/app chunks stay well under 1MB after manualChunks + lazy panels.
    chunkSizeWarningLimit: 4000,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return
          if (
            id.includes("monaco-editor") ||
            id.includes("@monaco-editor")
          ) {
            return "monaco"
          }
          if (id.includes("@xterm")) return "xterm"
          if (
            id.includes("react-markdown") ||
            id.includes("remark-") ||
            id.includes("rehype-") ||
            id.includes("highlight.js") ||
            id.includes("/highlight.js/")
          ) {
            return "markdown"
          }
          if (id.includes("@tanstack")) return "tanstack"
          if (id.includes("@base-ui")) return "base-ui"
          if (id.includes("lucide-react")) return "lucide"
          if (id.includes("zustand")) return "zustand"
          if (id.includes("@tauri-apps")) return "tauri"
          if (
            id.includes("node_modules/react/") ||
            id.includes("node_modules/react-dom/") ||
            id.includes("node_modules/scheduler/")
          ) {
            return "react-vendor"
          }
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    // false (not true): a stale Vite listener left on 1420 (e.g. from a
    // crashed `tauri dev`) used to hard-fail every subsequent `tauri dev`
    // with EADDRINUSE. Falling forward to the next free port lets dev
    // continue immediately; chosen over a predev kill-port script, which
    // risks killing an unrelated process that happens to hold 1420.
    strictPort: false,
    // Bind IPv4 explicitly. Vite's default `localhost` can listen on ::1 only;
    // WKWebView often resolves localhost → 127.0.0.1 and shows a blank window.
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}))
