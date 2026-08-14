import { expect, test, chromium, type BrowserContext, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const repoRoot = path.resolve(process.cwd(), "..");
const cdpPort = Number(process.env.SGT_WEBVIEW2_CDP_PORT ?? "9333");
const cdpUrl = process.env.SGT_WEBVIEW2_CDP_URL ?? `http://127.0.0.1:${cdpPort}`;
let launchedApp: ChildProcess | null = null;
let launchError: string | null = null;

const coldFirstUseTimeoutMs = 240_000;

test.setTimeout(coldFirstUseTimeoutMs + 60_000);

async function waitForCdpEndpoint(url: string, timeoutMs: number) {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown = null;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${url}/json/version`);
      if (response.ok) return;
      lastError = new Error(`CDP version status ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw lastError instanceof Error ? lastError : new Error(`Timed out waiting for ${url}`);
}

async function findScreenRecordPage(context: BrowserContext, timeoutMs: number): Promise<Page | null> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const page of context.pages()) {
      const title = await page.title().catch(() => "");
      if (title.includes("SGT Record") || page.url().startsWith("screenrecord://")) {
        return page;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return null;
}

async function ensureDebugAppLaunched() {
  if (process.env.SGT_WEBVIEW2_CDP_URL) {
    await waitForCdpEndpoint(cdpUrl, coldFirstUseTimeoutMs);
    return;
  }

  const exePath = path.join(
    repoRoot,
    "target",
    "debug",
    "screen-goated-toolbox.exe",
  );
  if (!fs.existsSync(exePath)) {
    launchError = `Missing ${exePath}. Run cargo build from the repository root first.`;
    return;
  }

  launchedApp = spawn(
    exePath,
    [
      "--screen-record-wry-smoke",
      "--screen-record-webview2-debug-port",
      String(cdpPort),
    ],
    {
      cwd: repoRoot,
      detached: true,
      env: {
        ...process.env,
        SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR: path.join(
          repoRoot,
          "target",
          "wry-smoke-webview2",
          String(cdpPort),
        ),
      },
      stdio: "ignore",
    },
  );
  launchedApp.unref();
  await waitForCdpEndpoint(cdpUrl, coldFirstUseTimeoutMs);
}

test.beforeAll(async () => {
  try {
    await ensureDebugAppLaunched();
  } catch (error) {
    launchError = error instanceof Error ? error.message : String(error);
  }
});

test.afterAll(() => {
  if (launchedApp?.pid) {
    try {
      process.kill(-launchedApp.pid);
    } catch {
      try {
        launchedApp.kill();
      } catch {
        // Best effort cleanup; externally supplied CDP sessions are not owned here.
      }
    }
  }
});

test("connects to the real Wry WebView2 shell over CDP", async () => {
  if (launchError) throw new Error(launchError);
  const browser = await chromium.connectOverCDP(cdpUrl);
  try {
    const context = browser.contexts()[0];
    expect(context, "WebView2 browser context should be available").toBeTruthy();
    const page = await findScreenRecordPage(context!, 15_000);
    expect(page, "WebView2 page should be available").toBeTruthy();
    await expect(page!.locator(".app-container")).toBeVisible();
    await expect.poll(() => page!.evaluate(() => Boolean((window as { isWry?: boolean }).isWry))).toBe(true);
    await expect.poll(() => page!.evaluate(async () => {
      const faces = await document.fonts.load("400 16px 'Google Sans Flex'");
      return faces.some((face) =>
        face.family.replaceAll("'", "") === "Google Sans Flex" && face.status === "loaded"
      );
    })).toBe(true);
  } finally {
    await browser.close();
  }
});
