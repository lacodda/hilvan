import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
// The import attribute is required by Vite's native config loader.
import pkg from './package.json' with { type: 'json' }

/**
 * Stamps the app's version into the service worker.
 *
 * Files in `public/` are copied verbatim, so `define` does not reach them and
 * the worker would ship with the placeholder in it - every deploy sharing one
 * shell cache, and the learner on an old build after an update. The
 * substitution happens on the built file, so `pnpm dev` serves the source as
 * it is written.
 */
function stampServiceWorker(): Plugin {
  return {
    name: 'hilvan-stamp-service-worker',
    apply: 'build',
    // After the public directory has been copied into the output.
    closeBundle() {
      const worker = fileURLToPath(new URL('./dist/sw.js', import.meta.url))
      const source = readFileSync(worker, 'utf8')
      const stamped = source.replace('__APP_VERSION__', pkg.version)
      if (stamped === source) {
        // A worker that did not take the version would cache under a name
        // that never changes, and no deploy would ever retire the old shell.
        throw new Error('sw.js has no __APP_VERSION__ to stamp; the shell cache would never be versioned')
      }
      writeFileSync(worker, stamped)
    },
  }
}

// Test configuration lives in vitest.config.ts: a `test` block here does not
// type-check against Vite's own config type.
export default defineConfig({
  plugins: [react(), tailwindcss(), stampServiceWorker()],
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
