import { expect, test, chromium, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const repoRoot = path.resolve(process.cwd(), "..");
const cdpPort = Number(process.env.SGT_WEBVIEW2_CDP_PORT ?? "9333");
const cdpUrl = process.env.SGT_WEBVIEW2_CDP_URL ?? `http://127.0.0.1:${cdpPort}`;
let launchedApp: ChildProcess | null = null;
let launchError: string | null = null;

test.setTimeout(120_000);

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

async function findScreenRecordPage(pages: Page[], timeoutMs: number): Promise<Page | null> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const page of pages) {
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
    await waitForCdpEndpoint(cdpUrl, 10_000);
    return;
  }

  const exePath = path.join(
    repoRoot,
    "target",
    "debug",
    "screen-goated-toolbox.exe",
  );
  if (!fs.existsSync(exePath)) {
    launchError = `Missing ${exePath}. Run ORT_SKIP_DOWNLOAD=1 cargo build --target x86_64-pc-windows-gnu first.`;
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
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdpPort} --remote-debugging-address=0.0.0.0`,
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
  await waitForCdpEndpoint(cdpUrl, 45_000);
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
  test.skip(!!launchError, launchError ?? undefined);
  const browser = await chromium.connectOverCDP(cdpUrl!);
  try {
    const context = browser.contexts()[0];
    const pages = context?.pages() ?? [];
    const firstPage = pages[0] ?? await context?.waitForEvent("page", { timeout: 5_000 });
    const page = await findScreenRecordPage(firstPage ? [firstPage, ...pages] : pages, 10_000);
    expect(page, "WebView2 page should be available").toBeTruthy();
    await expect(page!.locator(".app-container")).toBeVisible();
    await expect.poll(() => page!.evaluate(() => Boolean((window as { isWry?: boolean }).isWry))).toBe(true);
  } finally {
    await browser.close();
  }
});
