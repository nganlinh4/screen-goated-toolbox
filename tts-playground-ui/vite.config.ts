import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Mirrors translation-gummy-ui: single bundled index.js + index.css under
// dist/assets/, so the Rust side can include_bytes! the three files and inline
// them into a single self-contained HTML page served through the shared font
// server.
export default defineConfig({
  plugins: [react()],
  build: {
    minify: "terser",
    terserOptions: { compress: { passes: 2 } },
    outDir: "dist",
    emptyOutDir: true,
    assetsDir: "assets",
    cssCodeSplit: false,
    rolldownOptions: {
      checks: { pluginTimings: false },
      input: "index.html",
      output: {
        entryFileNames: "assets/index.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: (assetInfo) => {
          if (assetInfo.name?.endsWith(".css")) {
            return "assets/index.css";
          }
          return "assets/[name][extname]";
        },
      },
    },
  },
});
