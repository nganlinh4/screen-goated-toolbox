import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

const cwd = process.cwd();
const packageJsonPath = path.join(cwd, 'package.json');

if (!fs.existsSync(packageJsonPath)) {
  process.exit(0);
}

const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const targetPackage = resolveRollupNativePackage();

if (!targetPackage) {
  process.exit(0);
}

const versionRange = packageJson.optionalDependencies?.[targetPackage];
if (!versionRange) {
  process.exit(0);
}

const requireFromPackage = createRequire(packageJsonPath);
if (hasNativeRollupPackage(requireFromPackage, targetPackage)) {
  process.exit(0);
}

console.log(`[ensure-rollup-native] Missing ${targetPackage}; installing ${versionRange}`);

const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const installResult = spawnSync(
  npmCommand,
  ['install', '--no-save', `${targetPackage}@${versionRange}`],
  {
    cwd,
    stdio: 'inherit',
    env: process.env,
  },
);

if (installResult.status !== 0) {
  process.exit(installResult.status ?? 1);
}

if (!hasNativeRollupPackage(requireFromPackage, targetPackage)) {
  console.error(`[ensure-rollup-native] ${targetPackage} is still unavailable after install`);
  process.exit(1);
}

function resolveRollupNativePackage() {
  if (process.platform === 'win32' && process.arch === 'x64') {
    return '@rollup/rollup-win32-x64-msvc';
  }
  if (process.platform === 'linux' && process.arch === 'x64') {
    return '@rollup/rollup-linux-x64-gnu';
  }
  return null;
}

function hasNativeRollupPackage(requireFromPackage, packageName) {
  try {
    requireFromPackage.resolve(`${packageName}/package.json`);
    return true;
  } catch {
    return false;
  }
}
