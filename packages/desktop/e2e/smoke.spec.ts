import { expect, test, type Page } from "@playwright/test"

const waitAppReady = async (page: Page) => {
  await page.waitForFunction(() => !document.body.innerText.includes("Loading…"), {
    timeout: 20_000,
  })
}

test.describe("browserMock smoke (PR gate)", () => {
  test("boots seeded chat shell", async ({ page }) => {
    await page.goto("/")
    await waitAppReady(page)

    await expect(page.getByRole("button", { name: /New Agent/i }).first()).toBeVisible()
    await expect(page.getByText("Tighten the chat shell spacing").first()).toBeVisible()
    await expect(page.getByRole("textbox", { name: "Message composer" })).toBeVisible()
  })

  test("opens settings and returns to chat", async ({ page }) => {
    await page.goto("/")
    await waitAppReady(page)

    await page.getByRole("button", { name: "Settings" }).click()
    await expect(page.getByText("General").first()).toBeVisible()
    await expect(page.getByRole("button", { name: "Models & Connections" })).toBeVisible()

    await page.getByRole("button", { name: "Back to chat" }).click()
    await expect(page.getByRole("textbox", { name: "Message composer" })).toBeVisible()
  })

  test("welcome route forces first-run when unconfigured", async ({ page }) => {
    await page.goto("/?welcome=1")
    await waitAppReady(page)

    await expect(page.getByText(/Add a provider key/i)).toBeVisible()
    await expect(page.getByRole("button", { name: "Continue" })).toBeVisible()
  })

  test("empty session hero is reachable from sidebar", async ({ page }) => {
    await page.goto("/")
    await waitAppReady(page)

    await page.getByRole("button", { name: /Session Empty session hero/i }).click()
    await expect(
      page.getByText(/Describe a task to start the native agent loop/i),
    ).toBeVisible()
  })

  test("send prompt streams mock assistant reply", async ({ page }) => {
    await page.goto("/")
    await waitAppReady(page)

    // Prefer a fresh draft so we don't send into a seeded transcript.
    await page.getByRole("button", { name: /New Agent/i }).first().click()
    const emptyHero = page.getByText(/Describe a task to start the native agent loop/i)
    if (!(await emptyHero.isVisible().catch(() => false))) {
      await page.getByRole("button", { name: /Session Empty session hero/i }).click()
    }
    await expect(emptyHero).toBeVisible()

    const composer = page.getByRole("textbox", { name: "Message composer" })
    await composer.fill("e2e: tighten spacing")
    await expect(page.getByRole("button", { name: "Send message" })).toBeEnabled()
    await page.getByRole("button", { name: "Send message" }).click()

    // browserMock streams tools + a reconnect demo before the reply (~5s).
    await expect(page.getByText("Preview mock reply — layout changes look good.")).toBeVisible({
      timeout: 30_000,
    })
  })
})
