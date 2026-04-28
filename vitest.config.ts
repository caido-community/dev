import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['./src/**/*.spec.ts', './playgrounds/**/*.spec.ts'],
    setupFiles: ['./playgrounds/setup.ts'],
  },
})
