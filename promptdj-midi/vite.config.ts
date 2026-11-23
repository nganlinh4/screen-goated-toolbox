import path from 'path';
import { defineConfig } from 'vite';


export default defineConfig({
  server: {
    port: 3037,
    host: '0.0.0.0',
    // Shared modules (ui-shared Material Symbols) live one level up.
    fs: { allow: ['..'] },
  },
  plugins: [],
  build: {
    minify: "oxc",
    rolldownOptions: {
      output: {
        entryFileNames: `assets/[name].js`,
        chunkFileNames: `assets/[name].js`,
        assetFileNames: `assets/[name].[ext]`,
        // Single bundle — no code splitting. Required for inlining into HTML
        // so the shared font server can serve everything as one page.
        manualChunks: undefined,
        codeSplitting: false,
      }
    }
  },
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, '.'),
    }
  }
});
