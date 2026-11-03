import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { cpSync, existsSync } from 'node:fs';

/** pdf.js needs its CMaps (CJK text) and standard fonts served as static files. */
function pdfjsAssets() {
  return {
    name: 'pdfjs-assets',
    buildStart() {
      for (const dir of ['cmaps', 'standard_fonts']) {
        const dest = `public/pdfjs/${dir}`;
        if (!existsSync(dest)) cpSync(`node_modules/pdfjs-dist/${dir}`, dest, { recursive: true });
      }
    },
  };
}

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [pdfjsAssets(), svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  build: {
    target: 'es2021',
    chunkSizeWarningLimit: 1500,
  },
});
