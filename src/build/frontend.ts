import { existsSync } from "fs";
import path from "path";

import { build, defineConfig, mergeConfig } from "vite";

import type { FrontendBuildOutput, FrontendPluginConfig } from "../types";
import { logInfo } from "../utils";

/**
 * Creates a Vite config for the frontend plugin.
 * @param cwd - The current working directory.
 * @param plugin - The frontend plugin configuration.
 * @returns The Vite config.
 */
function createViteConfig(cwd: string, plugin: FrontendPluginConfig) {
  // Set the entry point
  const root = path.resolve(cwd, plugin.root);
  const baseConfig = defineConfig({
    root,
    build: {
      outDir: "dist",
      emptyOutDir: true,
      lib: {
        entry: "src/index.ts",
        formats: ["es"],
        fileName: () => "index.js",
        cssFileName: "index",
      },
    },
    define: { "process.env.NODE_ENV": '"production"' },
  });

  return mergeConfig(baseConfig, plugin.vite ?? {});
}

/**
 * Builds the frontend plugin.
 * @param cwd - The current working directory.
 * @param pluginConfig - The frontend plugin configuration.
 * @returns The build output.
 */
export async function buildFrontendPlugin(
  cwd: string,
  pluginConfig: FrontendPluginConfig,
): Promise<FrontendBuildOutput> {
  const pluginRoot = path.resolve(cwd, pluginConfig.root);

  logInfo(`Building frontend plugin: ${pluginRoot}`);
  const viteConfig = createViteConfig(cwd, pluginConfig);
  await build(viteConfig);

  const hasCss = existsSync(path.join(pluginRoot, "dist", "index.css"));
  logInfo("Frontend plugin built successfully");

  return {
    kind: "frontend",
    id: pluginConfig.id,
    name: pluginConfig.name ?? "frontend",
    fileName: path.join(pluginRoot, "dist", "index.js"),
    cssFileName: hasCss
      ? path.join(pluginRoot, "dist", "index.css")
      : undefined,
    backendId: pluginConfig.backend?.id,
    assets: pluginConfig.assets ?? [],
  };
}
