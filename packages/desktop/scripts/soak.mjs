#!/usr/bin/env node
/**
 * Nightly soak skeleton (phase 3.2).
 *
 * Drives N browserMock turns against a running Vite preview and records a
 * lightweight memory sample between turns. Not a multi-hour soak yet — CI
 * nightly runs a short N (default 5); bump SOAK_TURNS locally for longer runs.
 *
 * Usage:
 *   # terminal A
 *   pnpm dev
 *   # terminal B
 *   pnpm soak                 # defaults: 5 turns, http://127.0.0.1:1420
 *   SOAK_TURNS=20 pnpm soak
 *
 * Env:
 *   SOAK_BASE_URL   preview URL (default http://127.0.0.1:1420)
 *   SOAK_TURNS      number of prompt turns (default 5)
 *   SOAK_OUT        optional JSON summary path
 */
import { chromium } from "@playwright/test"
import { writeFile } from "node:fs/promises"
import { mkdir } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const base = process.env.SOAK_BASE_URL ?? "http://127.0.0.1:1420"
const turns = Math.max(1, Number(process.env.SOAK_TURNS ?? 5))
const outPath =
  process.env.SOAK_OUT ??
  path.join(__dirname, "..", ".soak", `summary-${Date.now()}.json`)

const sampleMemory = async (page) => {
  return page.evaluate(() => {
    const perf = performance.memory
    return {
      jsHeapUsedMb: perf ? Math.round((perf.usedJSHeapSize / 1_048_576) * 10) / 10 : null,
      jsHeapTotalMb: perf ? Math.round((perf.totalJSHeapSize / 1_048_576) * 10) / 10 : null,
      domNodes: document.querySelectorAll("*").length,
      ts: Date.now(),
    }
  })
}

const waitApp = async (page) => {
  await page.waitForFunction(() => !document.body.innerText.includes("Loading…"), {
    timeout: 20_000,
  })
}

const browser = await chromium.launch({ headless: true })
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } })

const samples = []
const startedAt = Date.now()

try {
  await page.goto(base, { waitUntil: "networkidle" })
  await waitApp()

  // Prefer the empty seeded session so each turn doesn't fight a busy timeline.
  const empty = page.getByRole("button", { name: /Session Empty session hero/i })
  if ((await empty.count()) > 0) {
    await empty.click()
  } else {
    await page.getByRole("button", { name: /New Agent/i }).first().click()
  }

  samples.push({ turn: 0, ...(await sampleMemory(page)) })

  for (let i = 1; i <= turns; i++) {
    const composer = page.getByRole("textbox", { name: "Message composer" })
    await composer.fill(`soak turn ${i}: noop probe`)
    await page.getByRole("button", { name: "Send message" }).click()
    await page.getByText("Preview mock reply — layout changes look good.").last().waitFor({
      timeout: 45_000,
    })
    // Let the mock finish turn_completed + UI settle before the next send.
    await page.waitForTimeout(800)
    samples.push({ turn: i, ...(await sampleMemory(page)) })
    console.log(`[soak] turn ${i}/${turns} ok heap=${samples.at(-1)?.jsHeapUsedMb ?? "n/a"}MB`)
  }
} finally {
  await browser.close()
}

const summary = {
  ok: true,
  turns,
  elapsedMs: Date.now() - startedAt,
  base,
  samples,
  note:
    "Placeholder soak: browserMock only. Extend with real Tauri + MockProvider / live providers for full phase 3.2.",
}

await mkdir(path.dirname(outPath), { recursive: true })
await writeFile(outPath, `${JSON.stringify(summary, null, 2)}\n`)
console.log(`[soak] wrote ${outPath}`)
console.log(JSON.stringify({ turns: summary.turns, elapsedMs: summary.elapsedMs }, null, 2))
