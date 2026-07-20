import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [{
    name: "trim-generated-trailing-whitespace",
    generateBundle(_options, bundle) {
      const clean = (value: string) => value.replace(/[ \t]+$/gm, "");
      for (const output of Object.values(bundle)) {
        if (output.type === "chunk") output.code = clean(output.code);
        else if (typeof output.source === "string") output.source = clean(output.source);
      }
    },
  }],
  server: { fs: { allow: [resolve(__dirname, "..")] } },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsDir: "assets",
    rollupOptions: {
      input: "index.html",
      output: {
        entryFileNames: "assets/index.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: (asset) => asset.name?.endsWith(".css") ? "assets/index.css" : "assets/[name][extname]",
      },
    },
  },
});
