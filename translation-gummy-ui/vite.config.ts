import { defineConfig } from "vite";

export default defineConfig({
  build: {
    minify: "oxc",
    outDir: "dist",
    emptyOutDir: true,
    assetsDir: "assets",
    rolldownOptions: {
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
