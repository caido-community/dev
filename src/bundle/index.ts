import fs from "node:fs/promises";
import path from "node:path";

import { validatePackageManifest } from "../manifest";
import type { BuildOutput, CaidoConfig } from "../types";
import { logInfo, logSuccess } from "../utils";
import { expandPackageAssets } from "../utils/cp";
import { wasm } from "../wasm";

import { encodeImage, readAsset } from "./readme-assets";

function normalized(file: string): string {
  return process.platform === "win32" ? file.toLowerCase() : file;
}

function contains(root: string, file: string): boolean {
  const relative = path.relative(normalized(root), normalized(file));
  return (
    relative !== ".." &&
    !relative.startsWith(`..${path.sep}`) &&
    !path.isAbsolute(relative)
  );
}

export async function bundlePackage(options: {
  cwd: string;
  buildOutputs: BuildOutput[];
  config: CaidoConfig;
}): Promise<void> {
  const { config, buildOutputs } = options;
  const cwd = await fs.realpath(options.cwd);
  const distDir = path.join(cwd, "dist");
  logInfo("Bundling plugin package");

  async function checkOutput(): Promise<void> {
    try {
      const stats = await fs.lstat(distDir);
      if (
        stats.isSymbolicLink() ||
        !stats.isDirectory() ||
        normalized(await fs.realpath(distDir)) !== normalized(distDir)
      ) {
        throw new Error(
          "Package output directory must be a real directory inside the project root",
        );
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    for (const plugin of config.plugins) {
      const root = path.resolve(cwd, plugin.root);
      const realRoot = await fs.realpath(root);
      if (contains(distDir, root) || contains(distDir, realRoot)) {
        throw new Error(
          "Plugin roots must not be inside the package output directory",
        );
      }
    }
  }
  await checkOutput();

  async function checkedSource(source: string): Promise<string> {
    const file = path.resolve(cwd, source);
    const stats = await fs.lstat(file);
    const realFile = await fs.realpath(file);
    if (
      stats.isSymbolicLink() ||
      !contains(cwd, file) ||
      !contains(cwd, realFile)
    ) {
      throw new Error(
        `Package source path escapes project root or is a symbolic link: ${source}`,
      );
    }
    return realFile;
  }

  async function readFile(source: string): Promise<Buffer> {
    try {
      return await fs.readFile(await checkedSource(source));
    } catch (error) {
      if (
        source === "README.md" &&
        (error as NodeJS.ErrnoException).code === "ENOENT"
      ) {
        throw new Error("README.md is required but not found in project root");
      }
      throw error;
    }
  }

  const result = await wasm.package_plugin(
    JSON.stringify({ config, buildOutputs }),
    readFile,
    (pattern) => expandPackageAssets(cwd, pattern, checkedSource),
    (pathname, url) => readAsset(cwd, pathname, url),
    encodeImage,
    validatePackageManifest,
  );
  for (const message of result.messages) logInfo(message);
  // All input reads, destination checks, validation, and ZIP creation finish before deletion.
  await checkOutput();
  await fs.rm(distDir, { recursive: true, force: true });
  const packageDir = path.join(distDir, "plugin_package");
  await fs.mkdir(packageDir, { recursive: true });
  for (const file of result.files) {
    const destination = path.join(packageDir, file.path);
    await fs.mkdir(path.dirname(destination), { recursive: true });
    await fs.writeFile(destination, file.bytes);
  }
  await fs.writeFile(path.join(distDir, "plugin_package.zip"), result.zip);
  logSuccess("Plugin package zip file created successfully");
}
