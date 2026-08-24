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
const targetPackage = resolveRolldownNativePackage();

if (!targetPackage) {
  process.exit(0);
}

const versionRange = packageJson.optionalDependencies?.[targetPackage];
if (!versionRange) {
  process.exit(0);
}

const requireFromPackage = createRequire(packageJsonPath);
if (hasNativeRolldownPackage(requireFromPackage, targetPackage)) {
  process.exit(0);
}

console.log(`[ensure-rolldown-native] Missing ${targetPackage}; installing ${versionRange}`);

const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const installResult = spawnSync(
  npmCommand,
  ['install', '--no-save', '--package-lock=false', `${targetPackage}@${versionRange}`],
  {
    cwd,
    stdio: 'inherit',
    env: process.env,
  },
);

if (installResult.status !== 0) {
  process.exit(installResult.status ?? 1);
}

if (!hasNativeRolldownPackage(requireFromPackage, targetPackage)) {
  console.error(`[ensure-rolldown-native] ${targetPackage} is still unavailable after install`);
  process.exit(1);
}

function resolveRolldownNativePackage() {
  if (process.platform === 'win32' && process.arch === 'x64') {
    return '@rolldown/binding-win32-x64-msvc';
  }
  if (process.platform === 'linux' && process.arch === 'x64') {
    return '@rolldown/binding-linux-x64-gnu';
  }
  return null;
}

function hasNativeRolldownPackage(requireFromPackage, packageName) {
  try {
    requireFromPackage.resolve(`${packageName}/package.json`);
    return true;
  } catch {
    return false;
  }
}
