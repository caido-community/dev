import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/adapter.ts"],
  format: ["esm"],
  target: "node20",
  dts: true,
  clean: true,
  splitting: false,
  bundle: true,
  minify: false,
  outDir: "dist",
  sourcemap: true,
});
