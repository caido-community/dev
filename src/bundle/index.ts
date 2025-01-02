import path from 'path';
import fs from 'fs/promises';
import { validateManifest } from '@caido/plugin-manifest';
import { createManifest } from '../manifest';
import { RootPackageJson } from '../types';
import type { BuildOutput } from '../types';
import JSZip from 'jszip';
import { addDirectoryToZip, logInfo } from '../utils';
import { bundleFrontendPlugin } from './frontend';
import { bundleBackendPlugin } from "./backend";

/**
 * Creates the dist directories.
 * @param cwd - The current working directory.
 * @returns The dist directory and the plugin package directory.
 */
async function createDistDirectories(cwd: string) {
  const distDir = path.resolve(cwd, 'dist');
  await fs.rm(distDir, { recursive: true, force: true });

  const pluginPackageDir = path.join(distDir, 'plugin_package');
  await fs.mkdir(pluginPackageDir, { recursive: true });

  return { distDir, pluginPackageDir };
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
    cwd: string,
    packageJson: RootPackageJson,
    buildOutputs: BuildOutput[]
}): Promise<void> {
  logInfo('Bundling plugin package');
  const { cwd, packageJson, buildOutputs } = options;

  // Create dist directories
  const { distDir, pluginPackageDir } = await createDistDirectories(cwd);

  // Create manifest
  const manifest = createManifest({ packageJson });

  // Copy build outputs to dist directory
  for (const buildOutput of buildOutputs) {
    switch (buildOutput.kind) {
      case "frontend":
        manifest.plugins.push(bundleFrontendPlugin(pluginPackageDir, buildOutput));
        break;
      case "backend":
        manifest.plugins.push(bundleBackendPlugin(pluginPackageDir, buildOutput));
        break;
    }
  }

  // Assert that the manifest is valid  
  if (!validateManifest(manifest)) {
    throw new Error('Manifest is not valid:' + JSON.stringify(manifest, null, 2));
  }

  // Write manifest to dist directory
  const manifestPath = path.join(pluginPackageDir, 'manifest.json');
  const manifestContent = JSON.stringify(manifest, null, 2);
  await fs.writeFile(manifestPath, manifestContent);

  // Generate zip file
  const zip = new JSZip();

  // Zip all the contents of the plugin package directory
  await addDirectoryToZip(zip, pluginPackageDir);

  // Write zip file to dist directory
  const zipPath = path.join(distDir, 'plugin_package.zip');
  const zipFile = await fs.open(zipPath, 'w');
  zip.generateNodeStream({ type: 'nodebuffer', streamFiles: true })
    .pipe(zipFile.createWriteStream())
    .on('finish', () => {
      console.log('Plugin package zip file created successfully');
    });
} 
