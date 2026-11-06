import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["./src/**/*.spec.ts", "./playgrounds/**/*.spec.ts", "./tests/**/*.spec.{ts,mjs}"],
    setupFiles: ["./playgrounds/setup.ts"],
  },
});
