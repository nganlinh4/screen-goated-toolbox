import { readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const excludedDirectories = new Set(['node_modules', 'target', 'dist', 'build']);
const suites = new Set(['all', 'unit', 'browser']);

export function discoverTests(root, suite = 'all') {
  if (!suites.has(suite)) throw new Error(`Unknown JavaScript suite: ${suite}`);
  const files = [];
  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const filename = path.join(directory, entry.name);
      if (entry.isDirectory() && !excludedDirectories.has(entry.name)) visit(filename);
      else if (entry.isFile() && /\.test\.[cm]?[jt]s$/.test(entry.name)) {
        const browser = /\.browser\.test\.[cm]?[jt]s$/.test(entry.name);
        if (suite === 'all' || browser === (suite === 'browser')) files.push(filename);
      }
    }
  }
  for (const directory of ['src', 'scripts/tests', 'tests']) visit(path.join(root, directory));
  return files.sort();
}

export function runTests(files, root, stdio = 'inherit') {
  if (files.length === 0) throw new Error('No JavaScript tests were selected');
  const env = { ...process.env };
  // A new coordinator must not inherit another coordinator's worker identity.
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(process.execPath, ['--test', ...files], { cwd: root, stdio, env });
  if (result.error) throw result.error;
  return result.status ?? 1;
}

function main(args) {
  let suite = 'all', list = false;
  while (args.length) {
    const argument = args.shift();
    if (argument === '--suite' && args.length) suite = args.shift();
    else if (argument === '--list') list = true;
    else throw new Error(`Unknown or incomplete argument: ${argument}`);
  }
  const files = discoverTests(repoRoot, suite);
  if (files.length === 0) throw new Error('No JavaScript tests were selected');
  if (list) {
    for (const file of files) console.log(path.relative(repoRoot, file).replaceAll('\\', '/'));
    return 0;
  }
  console.log(`Running ${files.length} JavaScript test files (${suite})`);
  return runTests(files, repoRoot);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
