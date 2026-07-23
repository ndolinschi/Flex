import { chromium } from "playwright"
import { mkdir } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const outDir = path.join(__dirname, "..", ".preview-shots")
const base = process.env.PREVIEW_URL ?? "http://127.0.0.1:1420"

await mkdir(outDir, { recursive: true })

const browser = await chromium.launch({
  executablePath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  headless: true,
  args: ["--disable-gpu", "--no-sandbox"],
})
const page = await browser.newPage({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 2,
})

const shot = async (name) => {
  const file = path.join(outDir, name)
  await page.screenshot({ path: file, fullPage: false })
  console.log(`wrote ${file}`)
}

const waitApp = async () => {
  await page.waitForFunction(() => !document.body.innerText.includes("Loading…"), {
    timeout: 15_000,
  })
}

await page.goto(base, { waitUntil: "networkidle" })
await waitApp()

await page.waitForSelector("text=Tighten the chat shell spacing", { timeout: 10_000 })
await page.waitForTimeout(500)
await shot("01-chat-filled.png")

await page.getByRole("button", { name: /Explored 8 files/i }).click()
await page.waitForSelector("text=packages/desktop/src/App.tsx", { timeout: 5_000 })
await page.getByRole("button", { name: /Edit Composer\.tsx/i }).click()
await page.waitForSelector("text=Updated Composer.tsx", { timeout: 5_000 })
await page.waitForTimeout(400)
await shot("01b-tools-expanded.png")

await page.getByRole("button", { name: /Explored 8 files/i }).click()
await page.getByRole("button", { name: /Edit Composer\.tsx/i }).click()

await page.getByRole("button", { name: /Session Empty session hero/i }).click()
await page.waitForSelector("text=Describe a task to start the native agent loop", {
  timeout: 10_000,
})
await page.waitForTimeout(500)
await shot("02-empty-hero.png")

await page.getByRole("button", { name: "Settings" }).click()
await page.waitForSelector("header >> text=Provider settings", { timeout: 10_000 })
await page.waitForTimeout(500)
await shot("03-settings.png")

const backVisible = await page.getByRole("button", { name: "Back to chat" }).count()
await page.getByRole("button", { name: "Back to chat" }).click()
await page.waitForSelector("text=Describe a task to start the native agent loop", {
  timeout: 10_000,
})

await page.goto(`${base}/?welcome=1`, { waitUntil: "networkidle" })
await waitApp()
await page.waitForSelector("text=Configure a provider to get started", { timeout: 10_000 })
await page.waitForTimeout(500)
await shot("04-welcome.png")

await page.goto(base, { waitUntil: "networkidle" })
await waitApp()
await page.getByRole("button", { name: /Session Visual balance preview/i }).click()
await page.waitForSelector("text=Explored 8 files", { timeout: 10_000 })

const exploreBtn = page.getByRole("button", { name: /Explored 8 files/i })
await exploreBtn.click()
const exploreExpanded = await page
  .locator("text=packages/desktop/src/App.tsx")
  .count()
await exploreBtn.click()

const checks = {
  newAgentButtons: await page.getByRole("button", { name: /New agent/i }).count(),
  settingsButtons: await page.getByRole("button", { name: "Settings" }).count(),
  sectionPlus: await page.locator('aside [aria-label="New session"]').count(),
  headerSettings: await page
    .locator("header")
    .getByRole("button", { name: "Settings" })
    .count(),
  backVisible,
  exploreExpandable: await exploreBtn.count(),
  exploreExpanded,
  filledMessage: await page.locator("text=Tighten the chat shell spacing").count(),
}

console.log(JSON.stringify(checks, null, 2))

await browser.close()
