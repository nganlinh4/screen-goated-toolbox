import { expect, test } from "@playwright/test";

test("loads the editor with mocked native IPC and synthetic fixture support", async ({ page }) => {
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();

  const summary = await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProject("small"));
  expect(summary).toMatchObject({
    projectId: "synthetic-small",
    subtitleCount: 12,
    narrationCount: 4,
    audioCount: 2,
  });

  const state = await page.evaluate(() => window.__SGT_TEST__?.getEditorState());
  expect(state?.projectId).toBe("synthetic-small");
});

test("keeps imported audio track label actions hidden until label hover", async ({ page }) => {
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();
  await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProject("small"));

  const label = page.locator(".timeline-label-imported-audio");
  const download = page.locator(".timeline-label-imported-audio .timeline-label-audio-download");
  const add = page.locator(".timeline-label-imported-audio-add");

  await expect(label).toBeVisible();
  await expect(download).toBeAttached();
  await expect(add).toBeAttached();
  await expect(download).toHaveCSS("opacity", "0");
  await expect(add).toHaveCSS("opacity", "0");

  await label.hover();
  await expect(download).toHaveCSS("opacity", "1");
  await expect(add).toHaveCSS("opacity", "1");
});

test("keeps the Device Audio delay popover open across its transparent hover bridge", async ({ page }) => {
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();
  await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProject("small"));

  const label = page.locator(".timeline-label-device-audio");
  const bridge = label.locator(".timeline-label-track-delay-hover-bridge");
  const popover = label.locator(".timeline-label-device-audio-delay-popover");

  await expect(label).toBeVisible();
  await expect(bridge).toBeAttached();
  await expect(popover).toHaveCSS("opacity", "0");
  await expect(bridge).toHaveCSS("pointer-events", "none");

  await label.hover();
  await expect(popover).toHaveCSS("opacity", "1");
  await expect(bridge).toHaveCSS("pointer-events", "auto");

  const bridgeBox = await bridge.boundingBox();
  const popoverBox = await popover.boundingBox();
  expect(bridgeBox).not.toBeNull();
  expect(popoverBox).not.toBeNull();
  if (!bridgeBox || !popoverBox) return;

  await page.mouse.move(
    bridgeBox.x + bridgeBox.width - 3,
    bridgeBox.y + 4,
    { steps: 8 },
  );
  await expect(popover).toHaveCSS("opacity", "1");

  await page.mouse.move(popoverBox.x + 8, popoverBox.y + 8, { steps: 8 });
  await expect(popover).toHaveCSS("opacity", "1");
});
