import { BuildOutput } from "../types";
import path from "path";
import fs from "fs";
import { defineFrontendPluginManifest } from "../manifest";

export function bundleFrontendPlugin(pluginPackageDir: string, buildOutput: BuildOutput) {
  // Create plugin directory
  const pluginDir = path.join(pluginPackageDir, buildOutput.id);
  fs.mkdirSync(pluginDir, { recursive: true });

  // Copy JS file
  const jsDestPath = path.join(pluginDir, path.basename(buildOutput.fileName));
  fs.copyFileSync(buildOutput.fileName, jsDestPath);
  const jsRelativePath = path.relative(pluginPackageDir, jsDestPath);

  // Copy CSS file if it exists
  let cssRelativePath: string | null = null;
  if (buildOutput.cssFileName) {
    const cssDestPath = path.join(pluginDir, path.basename(buildOutput.cssFileName));
    fs.copyFileSync(buildOutput.cssFileName, cssDestPath);
    cssRelativePath = path.relative(pluginPackageDir, cssDestPath);
  }

  return defineFrontendPluginManifest({
    id: buildOutput.id,
    kind: 'frontend',
    name: buildOutput.name ?? buildOutput.id,
    js: jsRelativePath,
    css: cssRelativePath,
    backend: buildOutput.backendId ? {
      id: buildOutput.backendId,
    } : null
  });
}