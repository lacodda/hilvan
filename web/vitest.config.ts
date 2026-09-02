import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

// Kept apart from vite.config.ts on purpose: a `test` block in the Vite
// config does not type-check against Vite's own config type in this line.
export default defineConfig({
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  test: {
    include: ['src/**/*.test.ts'],
  },
})
