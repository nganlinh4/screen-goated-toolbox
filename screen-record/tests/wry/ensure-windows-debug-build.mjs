import { spawnSync } from "node:child_process";
import path from "node:path";

const repoRoot = path.resolve(process.cwd(), "..");
const result = spawnSync(
  "cargo",
  ["build", "--locked", "--bin", "screen-goated-toolbox"],
  {
    cwd: repoRoot,
    stdio: "inherit",
  },
);

process.exit(result.status ?? 1);
