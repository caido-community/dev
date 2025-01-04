import { defineConfig } from 'tsup'

export default defineConfig({
  entry: ['src/cli.ts', 'src/index.ts'],
  format: ['esm'],
  target: 'es2020',
  dts: true,
  clean: true,
  splitting: false,
  bundle: true,
  minify: false,
  outDir: 'dist',
  sourcemap: true,
}) 