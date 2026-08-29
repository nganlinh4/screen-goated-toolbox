import { defineConfig } from "vite";
import { homedir } from "node:os";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [{
    name: "trim-generated-trailing-whitespace",
    generateBundle(_options, bundle) {
      const clean = (value: string) => value.replace(/[ \t]+$/gm, "").replace(/^ +(?=\t)/gm, "");
      for (const output of Object.values(bundle)) {
        if (output.type === "chunk") output.code = clean(output.code);
        else if (typeof output.source === "string") output.source = clean(output.source);
      }
    },
  }],
  server: {
    fs: {
      allow: [resolve(import.meta.dirname, ".."), resolve(homedir(), "Downloads")],
    },
  },
  build: {
    minify: "terser",
    terserOptions: { compress: { passes: 2 } },
    outDir: "../src/overlay/image_creator/dist",
    emptyOutDir: true,
    assetsDir: "assets",
    rolldownOptions: {
      checks: { pluginTimings: false },
      input: "index.html",
      output: {
        entryFileNames: "assets/index.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: (assetInfo) =>
          assetInfo.name?.endsWith(".css") ? "assets/index.css" : "assets/[name][extname]",
      },
    },
  },
});
