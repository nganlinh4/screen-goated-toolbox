import { chromium } from "@playwright/test";
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const repoRoot = path.resolve(process.cwd(), "..");
const cdpPort = Number(process.env.SGT_WEBVIEW2_CDP_PORT ?? "9348");
const cdpUrl = `http://127.0.0.1:${cdpPort}`;
const exePath = path.join(repoRoot, "target", "debug", "screen-goated-toolbox.exe");
const dataDir = path.join(repoRoot, "target", "wry-step-audio-check", String(cdpPort));
const screenshotPath =
  process.env.SGT_STEP_AUDIO_WRY_SCREENSHOT ??
  "C:\\Users\\user\\AppData\\Local\\Temp\\sgt-step-audio-wry-narration.png";

function readWavSummary(filePath) {
  const buffer = fs.readFileSync(filePath);
  if (buffer.toString("ascii", 0, 4) !== "RIFF" || buffer.toString("ascii", 8, 12) !== "WAVE") {
    throw new Error(`Not a WAV file: ${filePath}`);
  }
  let offset = 12;
  let sampleRate = 0;
  let channels = 0;
  let bitsPerSample = 0;
  let dataBytes = 0;
  while (offset + 8 <= buffer.length) {
    const id = buffer.toString("ascii", offset, offset + 4);
    const size = buffer.readUInt32LE(offset + 4);
    const dataOffset = offset + 8;
    if (id === "fmt ") {
      channels = buffer.readUInt16LE(dataOffset + 2);
      sampleRate = buffer.readUInt32LE(dataOffset + 4);
      bitsPerSample = buffer.readUInt16LE(dataOffset + 14);
    } else if (id === "data") {
      dataBytes = size;
    }
    offset = dataOffset + size + (size % 2);
  }
  const bytesPerSecond = sampleRate * channels * (bitsPerSample / 8);
  return {
    filePath,
    size: buffer.length,
    sampleRate,
    channels,
    bitsPerSample,
    dataBytes,
    durationSec: bytesPerSecond > 0 ? dataBytes / bytesPerSecond : 0,
  };
}

function stopExistingApp() {
  spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      "Get-Process screen-goated-toolbox -ErrorAction SilentlyContinue | Stop-Process -Force",
    ],
    { cwd: repoRoot, stdio: "ignore" },
  );
}

async function waitForEndpoint(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${cdpUrl}/json/version`);
      if (response.ok) return;
      lastError = new Error(`CDP version status ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw lastError ?? new Error("Timed out waiting for WebView2 CDP endpoint");
}

stopExistingApp();
const app = spawn(
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
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
        `--remote-debugging-port=${cdpPort} --remote-debugging-address=0.0.0.0`,
      SGT_SCREEN_RECORD_WEBVIEW2_DATA_DIR: dataDir,
      SGT_SCREEN_RECORD_TEST_HARNESS: "1",
    },
    stdio: "ignore",
  },
);
app.unref();

let browser;
try {
  await waitForEndpoint(45_000);
  browser = await chromium.connectOverCDP(cdpUrl);
  const context = browser.contexts()[0];
  const page = context.pages()[0] ?? await context.waitForEvent("page", { timeout: 10_000 });
  const consoleMessages = [];
  page.on("console", (message) => {
    consoleMessages.push({
      type: message.type(),
      text: message.text(),
    });
  });
  page.on("pageerror", (error) => {
    consoleMessages.push({
      type: "pageerror",
      text: error.message,
    });
  });
  await page.waitForLoadState("domcontentloaded", { timeout: 15_000 }).catch(() => {});
  try {
    await page.waitForSelector(".app-container", { timeout: 30_000 });
  } catch (error) {
    const diagnostics = await page.evaluate(() => ({
      url: location.href,
      title: document.title,
      readyState: document.readyState,
      bodyText: document.body?.innerText?.slice(0, 1000) ?? "",
      rootHtml: document.querySelector("#root")?.innerHTML?.slice(0, 1000) ?? null,
      scripts: [...document.scripts].map((script) => script.src || script.textContent?.slice(0, 120)),
    })).catch((evalError) => ({ evalError: String(evalError) }));
    console.error(JSON.stringify({ ok: false, stage: "app-container", diagnostics, consoleMessages }, null, 2));
    throw error;
  }
  await page.waitForFunction(() => Boolean(window.__SGT_TEST__), null, { timeout: 30_000 });

  const loadedSummary = await page.evaluate(() =>
    window.__SGT_TEST__.loadSyntheticProjectWithOptions({
      profile: "small",
      subtitleCount: 1,
      narrationCount: 0,
      audioCount: 0,
      durationSec: 4,
    }),
  );
  await page.waitForFunction(
    () => window.__SGT_TEST__?.getEditorState().projectId === "synthetic-small",
    null,
    { timeout: 10_000 },
  );
  await page.waitForSelector(".side-panel", { timeout: 10_000 });
  try {
    await page
      .locator("button.panel-tab-button")
      .filter({ hasText: /Narr\.?|Narration|Thuyết Minh|T\.Minh|내레이션/ })
      .click();
  } catch (error) {
    const diagnostics = await page.evaluate(() => ({
      editorState: window.__SGT_TEST__?.getEditorState(),
      tabTexts: [...document.querySelectorAll("button.panel-tab-button")].map((button) =>
        button.textContent?.replace(/\s+/g, " ").trim(),
      ),
      sidePanelText: document.querySelector(".side-panel")?.textContent?.replace(/\s+/g, " ").trim().slice(0, 1000),
    })).catch((evalError) => ({ evalError: String(evalError) }));
    console.error(JSON.stringify({ ok: false, stage: "narration-tab", diagnostics, consoleMessages }, null, 2));
    throw error;
  }
  await page.waitForSelector(".narration-panel", { timeout: 10_000 });
  await page.locator(".narration-method-select").click();
  await page.locator(".panel-select-option").filter({ hasText: "Step Audio EditX" }).click();
  await page.waitForSelector(".narration-panel-step-audio-voices", { timeout: 10_000 });
  const addPromptVoice = page.locator(".narration-step-audio-voice-add");
  if (await addPromptVoice.isVisible({ timeout: 1_000 }).catch(() => false)) {
    await addPromptVoice.click();
    const cantoneseOption = page.locator(".panel-select-option").filter({ hasText: "Cantonese" });
    if (await cantoneseOption.isVisible({ timeout: 5_000 })) {
      await cantoneseOption.click();
    }
  }
  await page.locator(".narration-step-audio-prompt").fill("Use a calm narration delivery.");
  await page.locator(".narration-panel-generate-button").click();
  await page.waitForFunction(
    () => (window.__SGT_TEST__?.getEditorState().narrationCount ?? 0) > 0,
    null,
    { timeout: 360_000 },
  );
  const generatedPaths = await page.evaluate(() => window.__SGT_TEST__?.getNarrationAudioPaths() ?? []);
  const generatedWavs = generatedPaths.map(readWavSummary);
  if (generatedWavs.length === 0) {
    throw new Error("Step Audio narration did not produce a WAV path");
  }
  for (const wav of generatedWavs) {
    if (wav.sampleRate !== 24_000 || wav.channels !== 1 || wav.bitsPerSample !== 16 || wav.durationSec < 0.45) {
      throw new Error(`Unexpected Step Audio WAV summary: ${JSON.stringify(wav)}`);
    }
  }

  const result = await page.evaluate(() => {
    const rows = [...document.querySelectorAll(".narration-panel-step-audio-voice-config")];
    const overflowing = [...document.querySelectorAll(".narration-panel, .narration-panel *")]
      .filter((el) => el instanceof HTMLElement)
      .filter((el) => el.scrollWidth > el.clientWidth + 2 && getComputedStyle(el).overflowX === "visible")
      .slice(0, 20)
      .map((el) => ({
        cls: String(el.className),
        text: el.textContent?.trim().slice(0, 80),
        scrollWidth: el.scrollWidth,
        clientWidth: el.clientWidth,
      }));
    return {
      url: location.href,
      title: document.title,
      editorState: window.__SGT_TEST__?.getEditorState(),
      methodLabel: document
        .querySelector(".narration-method-select .panel-select-trigger-label")
        ?.textContent?.trim(),
      rowTexts: rows.map((row) => row.textContent?.replace(/\s+/g, " ").trim()),
      promptValue: document.querySelector(".narration-step-audio-prompt")?.value,
      statusText: document.querySelector(".narration-panel-status")?.textContent?.replace(/\s+/g, " ").trim(),
      overflowing,
    };
  });
  await page.locator(".narration-panel").screenshot({ path: screenshotPath });
  console.log(JSON.stringify({ ok: true, loadedSummary, result, generatedWavs, screenshotPath }, null, 2));
} finally {
  if (browser) await browser.close().catch(() => {});
  try {
    process.kill(-app.pid);
  } catch {
    try {
      app.kill();
    } catch {
      // Best effort cleanup.
    }
  }
  stopExistingApp();
}
