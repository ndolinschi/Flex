import { expect, test } from "@playwright/test"

test.describe("native-app gate (PR gate)", () => {
  test("vite preview shows desktop app required", async ({ page }) => {
    await page.goto("/")
    await expect(page.getByText("Desktop app required")).toBeVisible({
      timeout: 20_000,
    })
    await expect(page.getByText(/pnpm tauri dev/i)).toBeVisible()
  })
})
