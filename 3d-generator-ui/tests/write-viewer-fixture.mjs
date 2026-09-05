import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { animatedModelFixture } from "./animated-model-fixture.ts";

if (!process.argv[2]) throw new Error("Provide a new GLB fixture output path");
const output = resolve(process.argv[2]);
if (!output.endsWith(".glb")) throw new Error("Fixture output must be a GLB");
await mkdir(dirname(output), { recursive: true });
await writeFile(output, animatedModelFixture(), { flag: "wx" });
console.log(output);
