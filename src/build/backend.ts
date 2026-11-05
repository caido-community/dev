import { builtinModules } from "module";
import path from "path";

import { build, type Options } from "tsup";

import type { BackendBuildOutput, BackendPluginConfig } from "../types";
import { logInfo, slash } from "../utils";

/**
 * Creates a tsup config for the backend plugin.
 * @param cwd - The current working directory.
 * @param plugin - The backend plugin configuration.
 * @returns The tsup config.
 */
function createTsupConfig(cwd: string, plugin: BackendPluginConfig): Options {
  const root = path.resolve(cwd, plugin.root);
  return {
    target: "esnext",
    entry: [slash(path.resolve(root, "src/index.ts"))],
    outDir: slash(path.resolve(root, "dist")),
    outExtension: () => {
      return { js: ".js" };
    },
    format: ["esm"],
    config: false,
    clean: true,
    sourcemap: false,
    external: [/caido:.+/, "sqlite", ...builtinModules],
  };
}

/**
 * Builds the backend plugin.
 * @param cwd - The current working directory.
 * @param pluginConfig - The backend plugin configuration.
 * @returns The build output.
 */
export async function buildBackendPlugin(
  cwd: string,
  pluginConfig: BackendPluginConfig,
): Promise<BackendBuildOutput> {
  const pluginRoot = path.resolve(cwd, pluginConfig.root);

  logInfo(`Building backend plugin: ${pluginRoot}`);
  const tsupConfig = createTsupConfig(cwd, pluginConfig);
  await build(tsupConfig);
  logInfo("Backend built successfully");

  return {
    kind: "backend",
    id: pluginConfig.id,
    name: pluginConfig.name ?? "backend",
    fileName: path.join(pluginRoot, "dist", "index.js"),
    assets: pluginConfig.assets ?? [],
  };
}
