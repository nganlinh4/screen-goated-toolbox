import { spawn, spawnSync } from "node:child_process";
import path from "node:path";

const repoRoot = path.resolve(process.cwd(), "..");
const cdpPort = Number(process.env.SGT_WEBVIEW2_CDP_PORT ?? "9333");
const exePath = path.join(repoRoot, "target", "debug", "screen-goated-toolbox.exe");
const dataDir = path.join(repoRoot, "target", "wry-smoke-webview2", String(cdpPort));

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `${command} failed`);
  }
  return typeof result.stdout === "string" ? result.stdout.trim() : "";
}

function powershell(script, options = {}) {
  return run("powershell.exe", ["-NoProfile", "-Command", script], options);
}

function ensureBuild() {
  run("node", [path.join("tests", "wry", "ensure-windows-debug-build.mjs")], {
    cwd: process.cwd(),
    stdio: "inherit",
  });
}

function stopExistingApp() {
  spawnSync("powershell.exe", [
    "-NoProfile",
    "-Command",
    "Get-Process screen-goated-toolbox -ErrorAction SilentlyContinue | Stop-Process -Force",
  ], { cwd: repoRoot, stdio: "ignore" });
}

function findRecordPage(pages) {
  return pages.find((page) =>
    page?.title === "SGT Record" ||
    String(page?.url ?? "").includes("screenrecord"),
  );
}

async function waitForRecordPage(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const raw = powershell(
        `Invoke-RestMethod http://127.0.0.1:${cdpPort}/json/list | ConvertTo-Json -Compress -Depth 5`,
      );
      const parsed = JSON.parse(raw);
      const pages = Array.isArray(parsed) ? parsed : parsed.value;
      if (Array.isArray(pages)) {
        const recordPage = findRecordPage(pages);
        if (recordPage) return recordPage;
        lastError = new Error(`SGT Record page not ready: ${JSON.stringify(pages)}`);
      } else {
        lastError = new Error("CDP returned no page array");
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw lastError ?? new Error("Timed out waiting for WebView2 CDP page list");
}

ensureBuild();
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
    },
    stdio: "ignore",
  },
);
app.unref();

try {
  const recordPage = await waitForRecordPage(90_000);
  if (!recordPage.webSocketDebuggerUrl) {
    throw new Error("SGT Record CDP page is missing webSocketDebuggerUrl");
  }
  console.log(`[WrySmoke] CDP page ready: ${recordPage.title} ${recordPage.url}`);
} finally {
  stopExistingApp();
}
