import { defineConfig } from "oxlint";

export default defineConfig({
  rules: {
    "oxc/no-accumulating-spread": "error",
  },
  ignorePatterns: ["**/dist/**", "**/node_modules/**", "**/coverage/**", "rust/target/**"],
});
