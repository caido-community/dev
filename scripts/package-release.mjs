#!/usr/bin/env node
/**
 * Stage universal and optional native npm packages from built release artifacts.
 * Usage (paths are relative to cwd):
 *   node scripts/package-release.mjs --target linux-x64 \
 *     --binary dist/native/caido-dev --out artifacts/release [--source <repository>]
 * Targets: linux-x64, linux-arm64, win32-x64, darwin-arm64.
 * The new output root contains main/ and native/ with the same source version.
 * No build, pack, install, download or publish is performed. Binary architecture
 * is the caller's responsibility. Before packaging, populate Cargo sources with:
 *   cargo fetch --locked --manifest-path rust/Cargo.toml
 * Notices are then collected with locked, offline Cargo metadata.
 */
import { realpathSync } from "node:fs";
import { chmod, copyFile, lstat, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { z } from "zod";
import { nativeTargets } from "./native-launcher.mjs";
import { collectNativeNotices } from "./native-notices.mjs";

const repository = fileURLToPath(new URL("../", import.meta.url));
const requiredDistFiles = ["cli.js", "index.js", "index.d.ts", "adapter.js"];
const optionalDistFiles = ["index.js.map", "adapter.js.map", "adapter.d.ts"];

function validateOutput(source, output) {
  const insideDist = relative(join(source, "dist"), output);
  if (
    insideDist === "" ||
    (!isAbsolute(insideDist) && insideDist !== ".." && !insideDist.startsWith(`..${sep}`))
  ) {
    throw new Error("Output must be outside the source dist directory.");
  }
}

async function loadSource(source = repository) {
  source = resolve(source);
  const manifest = z
    .object({
      name: z.literal("@caido-community/dev"),
      version: z.string().min(1),
      license: z.literal("MIT"),
      files: z.tuple([z.literal("dist")]),
    })
    .passthrough()
    .parse(JSON.parse(await readFile(join(source, "package.json"), "utf8")));
  const license = await readFile(join(source, "LICENSE"));
  return { source, manifest, license };
}

async function stageMain({ source, manifest, license }, output) {
  validateOutput(source, output);
  const dist = join(source, "dist");
  if (!(await lstat(dist)).isDirectory()) throw new Error("Expected a built dist directory.");
  const files = [];
  for (const file of [...requiredDistFiles, ...optionalDistFiles]) {
    let entry;
    try {
      entry = await lstat(join(dist, file));
    } catch (error) {
      if (error.code === "ENOENT" && optionalDistFiles.includes(file)) continue;
      throw error;
    }
    if (!entry.isFile()) throw new Error(`Expected a regular dist/${file} file.`);
    files.push(file);
  }
  if (!(await lstat(join(source, "README.md"))).isFile()) throw new Error("Expected README.md.");
  const staged = {
    ...manifest,
    optionalDependencies: Object.fromEntries(
      Object.values(nativeTargets).map((name) => [name, manifest.version]),
    ),
  };
  await mkdir(dirname(output), { recursive: true });
  await mkdir(output); // Refuse existing directories, files and symlinks.
  await mkdir(join(output, "dist"));
  for (const file of files) await copyFile(join(dist, file), join(output, "dist", file));
  await copyFile(join(source, "README.md"), join(output, "README.md"));
  await writeFile(join(output, "LICENSE"), license);
  await writeFile(join(output, "package.json"), `${JSON.stringify(staged, null, 2)}\n`);
  return staged;
}

export async function packageMain({ source = repository, out } = {}) {
  if (!out) throw new Error("Required: out <new directory>.");
  return stageMain(await loadSource(source), resolve(out));
}

async function stageNative({ manifest: metadata, license }, { target, binary, out }) {
  if (!Object.hasOwn(nativeTargets, target) || !binary || !out) {
    throw new Error(`Required: target (${Object.keys(nativeTargets).join(", ")}), binary and out.`);
  }
  binary = resolve(binary);
  if (!(await stat(binary)).isFile()) throw new Error(`Not a binary file: ${binary}`);
  // Fail before creating output if any resolved dependency's notices are missing.
  const notices = await collectNativeNotices();
  const output = resolve(out);
  const [os, cpu] = target.split("-");
  const filename = os === "win32" ? "caido-dev.exe" : "caido-dev";
  const manifest = {
    name: nativeTargets[target],
    version: metadata.version,
    description: `Native caido-dev executable for ${target}`,
    license: "MIT",
    os: [os],
    cpu: [cpu],
    files: ["bin", "LICENSE", "THIRD_PARTY_NOTICES"],
    // npm preserves executable permissions for declared bin entries when packing.
    bin: { [`caido-dev-${target}`]: `bin/${filename}` },
    publishConfig: { access: "public" },
  };
  if (metadata.repository) manifest.repository = metadata.repository;
  await mkdir(dirname(output), { recursive: true });
  await mkdir(output); // Refuse existing output, including symlinks.
  await mkdir(join(output, "bin"));
  const destination = join(output, "bin", filename);
  await copyFile(binary, destination);
  await chmod(destination, 0o755);
  await writeFile(join(output, "LICENSE"), license);
  await writeFile(join(output, "THIRD_PARTY_NOTICES"), notices);
  await writeFile(join(output, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

export async function packageNative({ source = repository, ...options } = {}) {
  return stageNative(await loadSource(source), options);
}

export async function packageRelease(args = process.argv.slice(2)) {
  const { values } = parseArgs({
    args,
    options: {
      target: { type: "string" },
      binary: { type: "string" },
      out: { type: "string" },
      source: { type: "string" },
    },
    allowPositionals: false,
  });
  if (!Object.hasOwn(nativeTargets, values.target) || !values.binary || !values.out) {
    throw new Error(
      `Required: --target (${Object.keys(nativeTargets).join(", ")}) --binary <prebuilt executable> --out <new root> [--source <repository>].`,
    );
  }
  const context = await loadSource(values.source);
  const output = resolve(values.out);
  validateOutput(context.source, output);
  await mkdir(dirname(output), { recursive: true });
  await mkdir(output); // Reserve a new root; never reuse existing files, directories or symlinks.
  const native = await stageNative(context, { ...values, out: join(output, "native") });
  const main = await stageMain(context, join(output, "main"));
  return { main, native };
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  packageRelease().catch((error) => {
    console.error(`package-release: ${error.message}`);
    process.exitCode = 1;
  });
}
