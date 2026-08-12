import { expect, test, type Page } from "@playwright/test";

async function createCanvasVideoUrl(page: Page) {
  return page.evaluate(async () => {
    const canvas = document.createElement("canvas");
    canvas.width = 1280;
    canvas.height = 720;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas context unavailable");

    const gradient = context.createLinearGradient(0, 0, canvas.width, canvas.height);
    gradient.addColorStop(0, "#176b87");
    gradient.addColorStop(1, "#0e2435");
    context.fillStyle = gradient;
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "#ffffff";
    context.font = "700 72px sans-serif";
    context.fillText("CROP FRAME", 74, 120);
    context.fillStyle = "#f5b942";
    context.fillRect(460, 190, 360, 360);

    const stream = canvas.captureStream(5);
    const recorder = new MediaRecorder(stream, { mimeType: "video/webm" });
    const chunks: Blob[] = [];
    recorder.ondataavailable = (event) => chunks.push(event.data);
    const stopped = new Promise<void>((resolve) => {
      recorder.onstop = () => resolve();
    });
    recorder.start();
    await new Promise((resolve) => window.setTimeout(resolve, 250));
    recorder.stop();
    await stopped;
    stream.getTracks().forEach((track) => track.stop());
    return URL.createObjectURL(new Blob(chunks, { type: "video/webm" }));
  });
}

async function openCropWorkspace(page: Page) {
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();
  await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProject("small"));
  const videoUrl = await createCanvasVideoUrl(page);
  await page.evaluate((url) => window.__SGT_TEST__?.setCurrentVideoSource(url), videoUrl);
  const cropButton = page.locator(".playback-crop-toggle-btn");
  await expect(cropButton).toBeVisible();
  await cropButton.click();
  await expect(page.locator(".crop-workspace")).toBeVisible();
  await expect(page.getByRole("button", { name: "Original, 1280×720" })).toBeVisible();
}

test("crop presets expose exact dimensions and remain usable across layouts", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await openCropWorkspace(page);

  const portrait = page.getByRole("button", { name: "Aspect ratio 9:16, 406×720" });
  await portrait.click();
  await expect(portrait).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".crop-ratio-selection-size")).toHaveText("406 × 720");

  await page.evaluate(() => {
    window.__SGT_TEST__?.setCurrentTime(1);
    document.querySelector(".crop-workspace-video")?.dispatchEvent(new Event("loadeddata"));
  });
  await expect(portrait).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".crop-ratio-active-label")).toContainText("9:16");

  const eastHandle = page.locator('.crop-workspace-handle[data-active="false"].cursor-e-resize');
  const eastBox = await eastHandle.boundingBox();
  expect(eastBox).not.toBeNull();
  if (eastBox) {
    await page.mouse.move(eastBox.x + eastBox.width / 2, eastBox.y + eastBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(eastBox.x + eastBox.width / 2 - 48, eastBox.y + eastBox.height / 2);
    await page.mouse.up();
  }
  await expect(page.locator(".crop-ratio-active-label")).toContainText("9:16");
  await expect(portrait).toHaveAttribute("aria-pressed", "true");
  const lockedSize = await page.locator(".crop-ratio-selection-size").innerText();
  const lockedDimensions = lockedSize.match(/(\d+) × (\d+)/);
  expect(lockedDimensions).not.toBeNull();
  if (lockedDimensions) {
    const [, width, height] = lockedDimensions.map(Number);
    expect(width / height).toBeCloseTo(9 / 16, 2);
    expect(lockedSize).not.toBe("406 × 720");
  }

  await page.getByRole("button", { name: "Unlock aspect ratio" }).click();
  const unlockedEastBox = await eastHandle.boundingBox();
  expect(unlockedEastBox).not.toBeNull();
  if (unlockedEastBox) {
    await page.mouse.move(
      unlockedEastBox.x + unlockedEastBox.width / 2,
      unlockedEastBox.y + unlockedEastBox.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      unlockedEastBox.x + unlockedEastBox.width / 2 + 48,
      unlockedEastBox.y + unlockedEastBox.height / 2,
    );
    await page.mouse.up();
  }
  await expect(page.locator(".crop-ratio-active-label")).toHaveText("Custom");

  await page.setViewportSize({ width: 900, height: 700 });
  await expect(page.locator(".crop-ratio-panel")).toBeVisible();
  const overflow = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
  }));
  expect(overflow.width).toBeLessThanOrEqual(overflow.viewport);

  await page.evaluate(() => {
    window.postMessage({ type: "sr-set-settings", theme: "light" }, "*");
  });
  await expect(page.locator("html")).not.toHaveClass(/dark/);
  await expect(page.locator(".crop-ratio-panel")).toBeVisible();
});
