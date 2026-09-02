import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath } from 'node:url'
// The import attribute is required by Vite's native config loader.
import pkg from './package.json' with { type: 'json' }

// Test configuration lives in vitest.config.ts: a `test` block here does not
// type-check against Vite's own config type.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: {
    port: 5173,
    // The API runs separately during development; proxying keeps the SPA on
    // same-origin paths, so no CORS handling is needed here or on the server.
    proxy: {
      '/api': 'http://127.0.0.1:8086',
    },
  },
})
