import path from "path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Temporary config to run the one-off golden generator (*.gen.ts).
export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  test: {
    environment: "node",
    globals: true,
    include: ["tests/unit/**/*.gen.ts"],
    pool: "threads",
  },
});
