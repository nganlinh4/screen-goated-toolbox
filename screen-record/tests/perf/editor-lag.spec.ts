import { expect, test } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const budgets = JSON.parse(
  fs.readFileSync(path.join(testDir, "budgets.json"), "utf8"),
) as {
  syntheticHuge: {
    loadProjectMs: number;
    dragP95FrameDeltaMs: number;
    wheelFirstVisualMs: number;
    wheelAnchorDriftPx: number;
    wheelP95FrameDeltaMs: number;
    wheelMaxLongTaskMs: number;
    projectsOpenMs: number;
    projectsOpenMaxLongTaskMs: number;
    maxLongTaskMs: number;
    longTaskCount: number;
    subtitleDomBlocks: number;
    audioDomBlocks: number;
    narrationDomBlocks: number;
    totalTimelineBlocks: number;
  };
};

async function loadHugeFixture(page: import("@playwright/test").Page) {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.stack || error.message);
  });
  await page.goto("/?sgtTestHarness=1");
  await expect(page.locator(".app-container")).toBeVisible();

  await page.evaluate(() => window.__SGT_TEST__?.resetPerf());
  await page.evaluate(() => window.__SGT_TEST__?.startAction("load:huge"));
  const loadStart = Date.now();
  const summary = await page.evaluate(() => window.__SGT_TEST__?.loadSyntheticProject("huge"));
  const loadProjectMs = Date.now() - loadStart;
  await page.evaluate(() => window.__SGT_TEST__?.endAction("load:huge"));

  expect(summary).toMatchObject({
    projectId: "synthetic-huge",
    subtitleCount: 10_000,
    narrationCount: 1_000,
    audioCount: 80,
  });
  try {
    await expect(page.locator(".timeline-scroll-viewport")).toBeVisible({ timeout: 20_000 });
  } catch (error) {
    throw new Error(`Timeline did not mount. Page errors:\n${pageErrors.slice(0, 5).join("\n\n")}`, {
      cause: error,
    });
  }

  return { loadProjectMs };
}

async function dragLane(page: import("@playwright/test").Page, selector: string, yRatio = 0.5) {
  const box = await page.locator(selector).boundingBox();
  expect(box).toBeTruthy();
  if (!box) return;
  await page.mouse.move(box.x + box.width * 0.32, box.y + box.height * yRatio);
  await page.mouse.down();
  for (let i = 0; i < 80; i += 1) {
    await page.mouse.move(box.x + box.width * (0.32 + i * 0.003), box.y + box.height * yRatio + (i % 7), { steps: 1 });
  }
  await page.mouse.up();
}

async function getTimelineCanvasWidth(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const canvas = document.querySelector<HTMLElement>(".timeline-canvas");
    return canvas?.getBoundingClientRect().width ?? 0;
  });
}

async function getPlayheadViewportX(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const viewport = document.querySelector<HTMLElement>(".timeline-scroll-viewport");
    const playhead = document.querySelector<HTMLElement>(".playhead");
    if (!viewport || !playhead) return null;
    return playhead.getBoundingClientRect().left - viewport.getBoundingClientRect().left;
  });
}

async function getWheelAnchorTime(page: import("@playwright/test").Page, xRatio = 0.55) {
  return page.evaluate((ratio) => {
    const viewport = document.querySelector<HTMLElement>(".timeline-scroll-viewport");
    const canvas = document.querySelector<HTMLElement>(".timeline-canvas");
    const duration = window.__SGT_TEST__?.getEditorState().duration ?? 0;
    if (!viewport || !canvas || duration <= 0) return 0;

    const bleedPx = 16;
    const rect = viewport.getBoundingClientRect();
    const visibleWidth = Math.max(viewport.clientWidth - bleedPx * 2, 1);
    const pointerContentX = Math.max(
      0,
      Math.min(visibleWidth, rect.width * ratio - bleedPx),
    );
    const contentWidth = Math.max(canvas.getBoundingClientRect().width, 1);
    return Math.max(
      0,
      Math.min(duration, ((viewport.scrollLeft + pointerContentX) / contentWidth) * duration),
    );
  }, xRatio);
}

async function dispatchWheelZoomAndMeasure(page: import("@playwright/test").Page, xRatio = 0.55) {
  return page.evaluate(async (ratio) => {
    const viewport = document.querySelector<HTMLElement>(".timeline-scroll-viewport");
    const canvas = document.querySelector<HTMLElement>(".timeline-canvas");
    const playhead = document.querySelector<HTMLElement>(".playhead");
    if (!viewport || !canvas || !playhead) {
      return { firstVisualMs: Number.POSITIVE_INFINITY, width: 0, playheadX: null };
    }

    const rect = viewport.getBoundingClientRect();
    const start = performance.now();
    viewport.dispatchEvent(
      new WheelEvent("wheel", {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width * ratio,
        clientY: rect.top + rect.height * 0.5,
        deltaY: -180,
      }),
    );
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

    return {
      firstVisualMs: performance.now() - start,
      width: canvas.getBoundingClientRect().width,
      playheadX: playhead.getBoundingClientRect().left - viewport.getBoundingClientRect().left,
    };
  }, xRatio);
}

async function wheelZoom(page: import("@playwright/test").Page) {
  const box = await page.locator(".timeline-scroll-viewport").boundingBox();
  expect(box).toBeTruthy();
  if (!box) return;
  await page.mouse.move(box.x + box.width * 0.55, box.y + box.height * 0.5);
  for (let i = 0; i < 12; i += 1) {
    await page.mouse.wheel(0, -180);
  }
}

async function openProjectsAndMeasure(page: import("@playwright/test").Page) {
  return page.evaluate(async () => {
    const button = document.querySelector<HTMLElement>(".projects-button");
    if (!button) return Number.POSITIVE_INFINITY;
    const start = performance.now();
    button.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    for (let i = 0; i < 12; i += 1) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      const view = document.querySelector<HTMLElement>(".projects-view");
      if (view && view.offsetParent !== null) {
        return performance.now() - start;
      }
    }
    return Number.POSITIVE_INFINITY;
  });
}

test("synthetic huge editor wheel zoom responds on the next frame", async ({ page }) => {
  await loadHugeFixture(page);
  const initialDom = await page.evaluate(() => window.__SGT_TEST__?.getDomStats());
  expect(initialDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);

  const initialWidth = await getTimelineCanvasWidth(page);
  const box = await page.locator(".timeline-scroll-viewport").boundingBox();
  expect(box).toBeTruthy();
  if (!box) return;
  const anchorTime = await getWheelAnchorTime(page, 0.55);
  await page.evaluate((time) => window.__SGT_TEST__?.setCurrentTime(time), anchorTime);
  await page.waitForTimeout(50);
  const playheadBeforeWheel = await getPlayheadViewportX(page);
  expect(playheadBeforeWheel).not.toBeNull();

  await page.evaluate(() => window.__SGT_TEST__?.resetPerf());
  await page.evaluate(() => window.__SGT_TEST__?.startPerfProbe());
  const firstWheel = await dispatchWheelZoomAndMeasure(page, 0.55);
  expect(firstWheel.width).toBeGreaterThan(initialWidth * 1.01);
  const firstVisualMs = firstWheel.firstVisualMs;
  const playheadAfterFirstWheel = firstWheel.playheadX;

  await page.mouse.move(box.x + box.width * 0.55, box.y + box.height * 0.5);
  for (let i = 0; i < 11; i += 1) {
    await page.mouse.wheel(0, -180);
  }
  await page.waitForTimeout(300);
  const frameProbe = await page.evaluate(() => window.__SGT_TEST__?.stopPerfProbe());
  const perf = await page.evaluate(() => window.__SGT_TEST__?.getPerfSnapshot());
  const finalDom = await page.evaluate(() => window.__SGT_TEST__?.getDomStats());
  const playheadAfterWheelSettled = await getPlayheadViewportX(page);

  expect(firstVisualMs).toBeLessThanOrEqual(budgets.syntheticHuge.wheelFirstVisualMs);
  expect(Math.abs((playheadAfterFirstWheel ?? 0) - (playheadBeforeWheel ?? 0))).toBeLessThanOrEqual(
    budgets.syntheticHuge.wheelAnchorDriftPx,
  );
  expect(Math.abs((playheadAfterWheelSettled ?? 0) - (playheadAfterFirstWheel ?? 0))).toBeLessThanOrEqual(
    budgets.syntheticHuge.wheelAnchorDriftPx,
  );
  expect(frameProbe?.p95FrameDeltaMs ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.wheelP95FrameDeltaMs);
  expect(Math.max(0, ...(perf?.longTasks.map((entry) => entry.duration) ?? []))).toBeLessThanOrEqual(
    budgets.syntheticHuge.wheelMaxLongTaskMs,
  );
  expect(finalDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);
});

test("synthetic huge editor fixture stays within attributed lag budget", async ({ page }) => {
  const { loadProjectMs } = await loadHugeFixture(page);

  const initialDom = await page.evaluate(() => window.__SGT_TEST__?.getDomStats());
  expect(initialDom?.subtitleBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.subtitleDomBlocks);
  expect(initialDom?.audioBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.audioDomBlocks);
  expect(initialDom?.narrationBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.narrationDomBlocks);
  expect(initialDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);

  await page.evaluate(() => window.__SGT_TEST__?.startPerfProbe());

  await page.evaluate(() => window.__SGT_TEST__?.startAction("wheel-zoom"));
  await wheelZoom(page);
  await page.evaluate(() => window.__SGT_TEST__?.endAction("wheel-zoom"));

  await page.evaluate(() => window.__SGT_TEST__?.startAction("drag:subtitle"));
  await dragLane(page, ".subtitle-track");
  await page.evaluate(() => window.__SGT_TEST__?.endAction("drag:subtitle"));

  await page.evaluate(() => window.__SGT_TEST__?.startAction("drag:audio"));
  await dragLane(page, ".audio-track");
  await page.evaluate(() => window.__SGT_TEST__?.endAction("drag:audio"));

  await page.evaluate(() => window.__SGT_TEST__?.startAction("drag:narration"));
  await dragLane(page, ".narration-track");
  await page.evaluate(() => window.__SGT_TEST__?.endAction("drag:narration"));

  await page.waitForTimeout(300);
  const frameProbe = await page.evaluate(() => window.__SGT_TEST__?.stopPerfProbe());
  const perf = await page.evaluate(() => window.__SGT_TEST__?.getPerfSnapshot());
  const finalDom = await page.evaluate(() => window.__SGT_TEST__?.getDomStats());

  expect(loadProjectMs).toBeLessThanOrEqual(budgets.syntheticHuge.loadProjectMs);
  expect(perf?.events.some((entry) => entry.label === "drag:subtitle" && typeof entry.duration === "number")).toBe(true);
  expect(perf?.renderCounters?.SubtitleTrack ?? 0).toBeGreaterThan(0);
  expect(frameProbe?.p95FrameDeltaMs ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.dragP95FrameDeltaMs);
  expect(Math.max(0, ...(perf?.longTasks.map((entry) => entry.duration) ?? []))).toBeLessThanOrEqual(
    budgets.syntheticHuge.maxLongTaskMs,
  );
  expect(perf?.longTasks.length ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.longTaskCount);
  expect(finalDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);
});

test("synthetic huge editor opens Projects without blocking the UI", async ({ page }) => {
  await loadHugeFixture(page);
  await page.evaluate(() => window.__SGT_TEST__?.resetPerf());
  await page.evaluate(() => window.__SGT_TEST__?.startPerfProbe());

  const openMs = await openProjectsAndMeasure(page);
  await expect(page.locator(".projects-view")).toBeVisible({
    timeout: budgets.syntheticHuge.projectsOpenMs,
  });

  await page.waitForTimeout(120);
  await page.evaluate(() => window.__SGT_TEST__?.stopPerfProbe());
  const perf = await page.evaluate(() => window.__SGT_TEST__?.getPerfSnapshot());

  expect(openMs).toBeLessThanOrEqual(budgets.syntheticHuge.projectsOpenMs);
  expect(Math.max(0, ...(perf?.longTasks.map((entry) => entry.duration) ?? []))).toBeLessThanOrEqual(
    budgets.syntheticHuge.projectsOpenMaxLongTaskMs,
  );
});
