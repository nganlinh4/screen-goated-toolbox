import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const MAX_SOURCE_LINES = 600;
const SOURCE_EXTENSIONS = new Set([".css", ".js", ".mjs", ".ts", ".tsx"]);
const roots = ["src", "tests", "scripts"];

async function collectSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...await collectSourceFiles(fullPath));
    } else if (SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
      files.push(fullPath);
    }
  }
  return files;
}

function countLines(source) {
  if (!source) return 0;
  const lines = source.split(/\r?\n/);
  return lines.at(-1) === "" ? lines.length - 1 : lines.length;
}

const files = (await Promise.all(roots.map(collectSourceFiles))).flat();
const violations = [];
for (const file of files) {
  const lines = countLines(await readFile(file, "utf8"));
  if (lines > MAX_SOURCE_LINES) {
    violations.push(`${file}: ${lines} lines`);
  }
}

if (violations.length > 0) {
  throw new Error(
    `Recorder source files must stay at or below ${MAX_SOURCE_LINES} lines:\n${violations.join("\n")}`,
  );
}
console.log(`Verified ${files.length} recorder source files (max ${MAX_SOURCE_LINES} lines).`);
