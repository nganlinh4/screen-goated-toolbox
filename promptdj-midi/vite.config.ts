import path from 'path';
import { defineConfig, loadEnv } from 'vite';


export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  return {
    server: {
      port: 3037,
      host: '0.0.0.0',
    },
    plugins: [],
    define: {
      'process.env.API_KEY': JSON.stringify(env.GEMINI_API_KEY),
      'process.env.GEMINI_API_KEY': JSON.stringify(env.GEMINI_API_KEY)
    },
    build: {
      rollupOptions: {
        output: {
          entryFileNames: `assets/[name].js`,
          chunkFileNames: `assets/[name].js`,
          assetFileNames: `assets/[name].[ext]`,
          // Single bundle — no code splitting. Required for inlining into HTML
          // so the shared font server can serve everything as one page.
          manualChunks: undefined,
          inlineDynamicImports: true,
        }
      }
    },
    resolve: {
      alias: {
        '@': path.resolve(__dirname, '.'),
      }
    }
  };
});
