import path from "path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
        protocol: "ws",
        host,
        port: 1421,
      }
      : undefined,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    rolldownOptions: {
      checks: {
        // Build duration varies with host load; correctness and bundle-size
        // warnings remain enabled while this nondeterministic advisory stays off.
        pluginTimings: false,
      },
      output: {
        manualChunks(id) {
          const normalizedId = id.replace(/\\/g, "/");
          if (normalizedId.includes("/src/lib/renderer/")) {
            return "editor-renderer";
          }
          if (!normalizedId.includes("node_modules")) return undefined;
          if (normalizedId.includes("motion") || normalizedId.includes("framer-motion")) {
            return "vendor-motion";
          }
          if (normalizedId.includes("@radix-ui")) return "vendor-radix";
          if (normalizedId.includes("react-dom") || normalizedId.includes("/react/")) {
            return "vendor-react";
          }
          return "vendor";
        },
        entryFileNames: `assets/[name].js`,
        chunkFileNames: `assets/[name].js`,
        assetFileNames: `assets/[name].[ext]`
      },
    },
    chunkSizeWarningLimit: 700,
  },
}));
