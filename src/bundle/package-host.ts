import fs from "node:fs/promises";
import path from "node:path";

import { z } from "zod";

import type { BuildOutput } from "../types";
import { expandPackageAssets } from "../utils/cp";
import { type PreloadedPackageInputs, type WasmPackageResult, wasm } from "../wasm";

import { checkedProjectPath, contains, normalized } from "./filesystem";
import { encodeImage, readAsset } from "./readme-assets";

const fileErrorSchema = z.object({ code: z.string() });

type PackageHost = {
  readFile: (source: string) => Promise<Buffer>;
  expandAssets: (pattern: string) => Promise<{ source: string; relative: string }[]>;
  readAsset: (pathname: string, url: string) => Promise<Buffer>;
  writePackage: (result: WasmPackageResult) => Promise<void>;
};

export async function preloadPackageInputs(
  host: PackageHost,
  buildOutputs: BuildOutput[],
): Promise<PreloadedPackageInputs> {
  const content = new TextDecoder("utf-8", { fatal: true }).decode(
    await host.readFile("README.md"),
  );
  const readme = await wasm.bundle_readme(content, host.readAsset, encodeImage);
  const files = new Map<string, Uint8Array>();
  const assets = new Map<string, { source: string; relative: string }[]>();
  async function preload(source: string): Promise<void> {
    if (!files.has(source)) files.set(source, await host.readFile(source));
  }
  for (const output of buildOutputs) {
    await preload(output.fileName);
    if (output.kind === "frontend" && output.cssFileName) await preload(output.cssFileName);
    for (const pattern of output.assets) {
      if (assets.has(pattern)) continue;
      const expanded = await host.expandAssets(pattern);
      assets.set(pattern, expanded);
      for (const file of expanded) await preload(file.source);
    }
  }
  return {
    files: [...files].map(([source, bytes]) => ({ source, bytes })),
    assets: [...assets].map(([pattern, matches]) => ({ pattern, files: matches })),
    readme,
  };
}

export async function createPackageHost(options: {
  cwd: string;
  pluginRoots: string[];
}): Promise<PackageHost> {
  const originalRoot = path.resolve(options.cwd);
  const cwd = await fs.realpath(originalRoot);
  const distDir = path.join(cwd, "dist");

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
      if (fileErrorSchema.safeParse(error).data?.code !== "ENOENT") throw error;
    }
    for (const pluginRoot of options.pluginRoots) {
      const root = path.resolve(cwd, pluginRoot);
      const realRoot = await fs.realpath(root);
      if (contains(distDir, root) || contains(distDir, realRoot)) {
        throw new Error("Plugin roots must not be inside the package output directory");
      }
    }
  }
  await checkOutput();

  async function checkedSource(source: string): Promise<string> {
    const file = path.resolve(cwd, source);
    const error = `Package source path escapes project root or is a symbolic link: ${source}`;
    const realFile = await checkedProjectPath(cwd, source, error);
    // Absolute build paths may use the project's original, non-canonical root.
    if (!contains(cwd, file) && !contains(originalRoot, file)) throw new Error(error);
    return realFile;
  }

  return {
    async readFile(source) {
      try {
        return await fs.readFile(await checkedSource(source));
      } catch (error) {
        if (source === "README.md" && fileErrorSchema.safeParse(error).data?.code === "ENOENT") {
          throw new Error("README.md is required but not found in project root");
        }
        throw error;
      }
    },
    expandAssets: (pattern) => expandPackageAssets(cwd, pattern, checkedSource),
    readAsset: (pathname, url) => readAsset(cwd, pathname, url),
    async writePackage(result) {
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
    },
  };
}
