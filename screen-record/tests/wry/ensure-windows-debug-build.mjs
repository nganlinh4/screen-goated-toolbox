import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const repoRoot = path.resolve(process.cwd(), "..");
const windowsExe = path.join(repoRoot, "target", "debug", "screen-goated-toolbox.exe");

if (fs.existsSync(windowsExe)) {
  process.exit(0);
}

const command = [
  "cd C:\\WORK\\screen-goated-toolbox",
  "cargo build",
].join("; ");

const result = spawnSync("powershell.exe", ["-NoProfile", "-Command", command], {
  cwd: repoRoot,
  stdio: "inherit",
});

process.exit(result.status ?? 1);
