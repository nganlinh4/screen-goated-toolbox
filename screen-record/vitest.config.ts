import path from "path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./tests/setup.ts"],
    include: ["tests/unit/**/*.test.{ts,tsx}", "tests/components/**/*.test.{ts,tsx}"],
    pool: "threads",
    maxWorkers: 2,
    minWorkers: 1,
    restoreMocks: true,
    clearMocks: true,
  },
});
