import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  root: resolve(__dirname, "viewer"),
  base: "./",
  publicDir: false,
  build: {
    outDir: resolve(__dirname, "viewer-dist/creation_model_viewer"),
    emptyOutDir: true,
    assetsDir: "assets",
    cssCodeSplit: false,
    sourcemap: false,
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      input: resolve(__dirname, "viewer/index.html"),
      output: {
        inlineDynamicImports: true,
        entryFileNames: "assets/viewer.js",
        chunkFileNames: "assets/forbidden-[name].js",
        assetFileNames: (assetInfo) => assetInfo.name?.endsWith(".css")
          ? "assets/viewer.css"
          : "assets/[name][extname]",
      },
    },
  },
});
