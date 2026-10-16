import { configs as deslopConfigs } from "ai-deslop";
import { defineConfig } from "oxlint";

export default defineConfig({
  jsPlugins: ["ai-deslop"],
  options: deslopConfigs.recommended.options,
  rules: {
    ...deslopConfigs.recommended.rules,
    "oxc/no-accumulating-spread": "error",
  },
  ignorePatterns: ["**/dist/**", "**/node_modules/**", "**/coverage/**", "rust/target/**"],
});
