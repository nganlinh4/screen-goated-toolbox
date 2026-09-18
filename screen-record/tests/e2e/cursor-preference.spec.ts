import { expect, test } from "@playwright/test";

test("explicit pointer choices persist while opening projects preserves the preference", async ({ page }) => {
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();
  await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProjectWithOptions({
    mousePositions: [{ x: 100, y: 100, timestamp: 0, isClicked: false }],
  }));
  await page.getByRole("tab", { name: "Cursor", exact: true }).click();
  const toggle = page.getByRole("switch", { name: "Use custom cursor" });
  await expect(toggle).toBeChecked();
  await toggle.click();
  await expect(toggle).not.toBeChecked();
  await expect.poll(() => page.evaluate(() => localStorage.getItem("screen-record-custom-cursor-pref-v1"))).toBe("0");
  await page.reload();
  await expect(page.locator(".app-container")).toBeVisible();
  await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProjectWithOptions({
    mousePositions: [{ x: 100, y: 100, timestamp: 0, isClicked: false }],
  }));
  await page.getByRole("tab", { name: "Cursor", exact: true }).click();
  await expect(toggle).toBeChecked();
  expect(await page.evaluate(() => localStorage.getItem("screen-record-custom-cursor-pref-v1"))).toBe("0");
  await toggle.click();
  await toggle.click();
  await expect.poll(() => page.evaluate(() => localStorage.getItem("screen-record-custom-cursor-pref-v1"))).toBe("1");
});
