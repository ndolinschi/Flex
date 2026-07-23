import path from "node:path"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vite"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST

const rootDir = path.dirname(fileURLToPath(import.meta.url))

const enableReactCompiler = true

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

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: false,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}))
