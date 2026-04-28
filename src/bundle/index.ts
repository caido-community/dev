import fs from "fs/promises";
import path from "path";

import { validateManifest } from "@caido/plugin-manifest";
import JSZip from "jszip";

import { createManifest } from "../manifest";
import type { BuildOutput, CaidoConfig } from "../types";
import { addDirectoryToZip, logInfo, logSuccess } from "../utils";

import { bundleBackendPlugin } from "./backend";
import { bundleFrontendPlugin } from "./frontend";
import { parseGitHubRepoInfo, transformReadmeImages } from "./readme-assets";

/**
 * Creates the dist directories.
 * @param cwd - The current working directory.
 * @returns The dist directory and the plugin package directory.
 */
async function createDistDirectories(cwd: string) {
  const distDir = path.resolve(cwd, "dist");
  await fs.rm(distDir, {
    recursive: true,
    force: true,
  });

  const pluginPackageDir = path.join(distDir, "plugin_package");
  await fs.mkdir(pluginPackageDir, { recursive: true });

  return {
    distDir,
    pluginPackageDir,
  };
}

/**
 * Bundles the plugin package.
 * @param options - The options.
 * @param options.cwd - The current working directory.
 * @param options.packageJson - The root package.json.
 * @param options.buildOutputs - The build outputs.
 * @returns The plugin package.
 */
export async function bundlePackage(options: {
  cwd: string;
  buildOutputs: BuildOutput[];
  config: CaidoConfig;
}): Promise<void> {
  logInfo("Bundling plugin package");
  const { cwd, buildOutputs, config } = options;

  // Create dist directories
  const { distDir, pluginPackageDir } = await createDistDirectories(cwd);

  // Copy README.md (always included, always at root)
  const readmePath = path.join(cwd, "README.md");
  try {
    await fs.access(readmePath);
  } catch {
    throw new Error("README.md is required but not found in project root");
  }

  // Read repository URL from project's package.json
  const packageJsonPath = path.join(cwd, "package.json");
  const packageJsonContent = await fs.readFile(packageJsonPath, "utf-8");
  const packageJson = JSON.parse(packageJsonContent);
  const repoUrl =
    typeof packageJson.repository === "string"
      ? packageJson.repository
      : packageJson.repository?.url;
  if (!repoUrl) {
    throw new Error("package.json must have a valid 'repository' field");
  }

  // Parse GitHub repo info (default to main branch)
  const repoInfo = parseGitHubRepoInfo(repoUrl, "main");

  // Transform README image links to GitHub raw URLs
  const transformedReadme = await transformReadmeImages(readmePath, repoInfo);

  // Write transformed README to package directory
  const readmeDest = path.join(pluginPackageDir, "README.md");
  await fs.writeFile(readmeDest, transformedReadme);

  // Create manifest
  const manifest = createManifest({ config });

  // Copy build outputs to dist directory
  for (const buildOutput of buildOutputs) {
    switch (buildOutput.kind) {
      case "frontend":
        manifest.plugins.push(
          bundleFrontendPlugin(cwd, pluginPackageDir, buildOutput),
        );
        break;
      case "backend":
        manifest.plugins.push(
          bundleBackendPlugin(cwd, pluginPackageDir, buildOutput),
        );
        break;
    }
  }

  // Assert that the manifest is valid
  if (!validateManifest(manifest)) {
    throw new Error(
      "Manifest is not valid:" + JSON.stringify(manifest, null, 2),
    );
  }

  // Write manifest to dist directory
  const manifestPath = path.join(pluginPackageDir, "manifest.json");
  const manifestContent = JSON.stringify(manifest, null, 2);
  await fs.writeFile(manifestPath, manifestContent);

  // Generate zip file
  const zip = new JSZip();

  // Zip all the contents of the plugin package directory
  await addDirectoryToZip(zip, pluginPackageDir);

  // Write zip file to dist directory
  const zipPath = path.join(distDir, "plugin_package.zip");
  const zipFile = await fs.open(zipPath, "w");
  await new Promise<void>((resolve, reject) => {
    zip
      .generateNodeStream({
        type: "nodebuffer",
        streamFiles: true,
      })
      .pipe(zipFile.createWriteStream())
      .on("finish", () => {
        logSuccess("Plugin package zip file created successfully");
        resolve();
      })
      .on("error", reject);
  });
}
