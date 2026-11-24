import { expect, test, type Page } from "@playwright/test";
import { syntheticVideoUrl } from "./fixtures/synthetic-video";

async function loadVideoProject(page: Page) {
  const videoUrl = await syntheticVideoUrl(page, "1920x1080");
  await page.evaluate(async (url) => {
    await window.__SGT_TEST__?.loadSyntheticProject("small");
    window.__SGT_TEST__?.setCurrentVideoSource(url);
  }, videoUrl);
}

test("remembers volume view and export choices across reopening and restart", async ({ page }, testInfo) => {
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();
  await loadVideoProject(page);
  const volume = page.getByRole("switch", { name: "Volume", exact: true });
  await expect(volume).not.toBeChecked();
  await volume.click();
  await expect(volume).toBeChecked();
  await page.locator(".header-export-button").click({ timeout: 5000 });
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: /^720p/ }).click();
  await expect(dialog.getByRole("button", { name: /^720p/ })).toHaveClass(/ui-choice-tile-active/);
  await dialog.getByRole("button", { name: "Standard", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Standard", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.screenshot({ path: testInfo.outputPath("export-defaults-dark.png") });
  await page.evaluate(() => window.postMessage({ type: "sr-set-settings", theme: "light" }, "*"));
  await expect(page.locator("html")).not.toHaveClass(/dark/);
  await page.screenshot({ path: testInfo.outputPath("export-defaults-light.png") });
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.locator(".header-export-button").click();
  await expect(dialog.getByRole("button", { name: /^720p/ })).toHaveClass(/ui-choice-tile-active/);
  await page.reload();
  await expect(page.locator(".app-container")).toBeVisible();
  await loadVideoProject(page);
  await expect(volume).toBeChecked();
  await page.locator(".header-export-button").click();
  await expect(dialog.getByRole("button", { name: /^720p/ })).toHaveClass(/ui-choice-tile-active/);
  await expect(dialog.getByRole("button", { name: "Standard", exact: true })).toHaveAttribute("aria-pressed", "true");
});
