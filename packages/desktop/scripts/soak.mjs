#!/usr/bin/env node
/**
 * Desktop perf soak — runs pure helper load tests via vitest (no Tauri).
 *
 * Usage: npm run soak  |  node scripts/soak.mjs
 * Env: SOAK_OUT=path  SOAK_ITERS is fixed in the test file (500).
 */
import { spawn } from "node:child_process"
import { mkdir, writeFile } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { performance } from "node:perf_hooks"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const root = path.join(__dirname, "..")
const outPath =
  process.env.SOAK_OUT ??
  path.join(root, ".soak", `summary-${Date.now()}.json`)

const t0 = performance.now()

const child = spawn(
  process.platform === "win32" ? "npx.cmd" : "npx",
  ["vitest", "run", "src/lib/perfSoak.test.ts", "--reporter=dot"],
  {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe"],
    env: process.env,
  },
)

let stdout = ""
let stderr = ""
child.stdout.on("data", (c) => {
  stdout += c
  process.stdout.write(c)
})
child.stderr.on("data", (c) => {
  stderr += c
  process.stderr.write(c)
})

const code = await new Promise((resolve) => {
  child.on("close", resolve)
})

const ms = performance.now() - t0
const passed = code === 0
const summary = {
  skipped: false,
  passed,
  exitCode: code,
  ms,
  at: new Date().toISOString(),
  suite: "src/lib/perfSoak.test.ts",
  note: "Pure helper soak (streaming store, windowing, patch, inflight, row index)",
  stdoutTail: stdout.slice(-2_000),
  stderrTail: stderr.slice(-1_000),
}

await mkdir(path.dirname(outPath), { recursive: true })
await writeFile(outPath, `${JSON.stringify(summary, null, 2)}\n`)
console.log(`\nsoak summary → ${outPath}`)
console.log(JSON.stringify({ passed, ms: Math.round(ms), exitCode: code }))
process.exit(code ?? 1)
