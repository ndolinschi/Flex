#!/usr/bin/env node
/**
 * Nightly soak skeleton (phase 3.2).
 *
 * The previous Vite + browserMock soak was removed — there is no mock IPC
 * backend. A real soak needs Tauri + provider harness; until that lands this
 * script exits 0 with a clear skip summary so nightly stays green.
 *
 * Usage:
 *   pnpm soak
 *
 * Env:
 *   SOAK_OUT        optional JSON summary path
 */
import { writeFile } from "node:fs/promises"
import { mkdir } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const outPath =
  process.env.SOAK_OUT ??
  path.join(__dirname, "..", ".soak", `summary-${Date.now()}.json`)

const summary = {
  skipped: true,
  reason:
    "browserMock soak removed — real Tauri + provider soak not implemented yet",
  at: new Date().toISOString(),
}

await mkdir(path.dirname(outPath), { recursive: true })
await writeFile(outPath, `${JSON.stringify(summary, null, 2)}\n`)
console.log(JSON.stringify(summary, null, 2))
