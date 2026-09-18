import { expect, test, type Page } from "@playwright/test";

async function loadVideoProject(page: Page) {
  await page.evaluate(async () => {
    await window.__SGT_TEST__?.loadSyntheticProject("small");
    const canvas = document.createElement("canvas");
    canvas.width = 1920;
    canvas.height = 1080;
    canvas.getContext("2d")!.fillRect(0, 0, canvas.width, canvas.height);
    const stream = canvas.captureStream(5);
    const recorder = new MediaRecorder(stream, { mimeType: "video/webm" });
    const chunks: Blob[] = [];
    recorder.ondataavailable = (event) => chunks.push(event.data);
    const stopped = new Promise<void>((resolve) => { recorder.onstop = () => resolve(); });
    recorder.start();
    await new Promise((resolve) => window.setTimeout(resolve, 250));
    recorder.stop();
    await stopped;
    stream.getTracks().forEach((track) => track.stop());
    window.__SGT_TEST__?.setCurrentVideoSource(URL.createObjectURL(new Blob(chunks, { type: "video/webm" })));
  });
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
