#!/usr/bin/env node
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
