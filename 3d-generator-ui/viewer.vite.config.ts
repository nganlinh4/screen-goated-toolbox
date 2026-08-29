import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  root: resolve(import.meta.dirname, "viewer"),
  base: "./",
  publicDir: false,
  build: {
    minify: "oxc",
    outDir: resolve(import.meta.dirname, "viewer-dist/creation_model_viewer"),
    emptyOutDir: true,
    assetsDir: "assets",
    cssCodeSplit: false,
    sourcemap: false,
    chunkSizeWarningLimit: 900,
    rolldownOptions: {
      input: resolve(import.meta.dirname, "viewer/index.html"),
      output: {
        codeSplitting: false,
        entryFileNames: "assets/viewer.js",
        chunkFileNames: "assets/forbidden-[name].js",
        assetFileNames: (assetInfo) => assetInfo.name?.endsWith(".css")
          ? "assets/viewer.css"
          : "assets/[name][extname]",
      },
    },
  },
});
