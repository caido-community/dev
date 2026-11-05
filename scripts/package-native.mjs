#!/usr/bin/env node
/**
 * Generate one optional native npm package from an already compiled executable.
 * Usage (paths are relative to cwd):
 *   node scripts/package-native.mjs --target linux-x64 \
 *     --binary target/x86_64-unknown-linux-gnu/release/caido-dev \
 *     --out artifacts/dev-linux-x64 [--package-json package.json] [--license LICENSE]
 * Targets: linux-x64, linux-arm64, win32-x64, darwin-arm64.
 * Version is read from --package-json (default: repository package.json).
 * --out must not already exist. Run once per target. No compile, pack, download,
 * or publish is performed. Binary architecture is the caller's responsibility.
 * Before packaging, populate all target source archives with:
 *   cargo fetch --locked --manifest-path rust/Cargo.toml
 * Notices are then collected with locked, offline Cargo metadata.
 */
import { chmod, copyFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { realpathSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { nativeTargets } from "./native-launcher.mjs";
import { collectNativeNotices } from "./native-notices.mjs";

export async function packageNative(args = process.argv.slice(2)) {
  const { values } = parseArgs({
    args,
    options: {
      target: { type: "string" },
      binary: { type: "string" },
      out: { type: "string" },
      "package-json": { type: "string" },
      license: { type: "string" },
    },
    allowPositionals: false,
  });
  const packageName = Object.hasOwn(nativeTargets, values.target)
    ? nativeTargets[values.target]
    : undefined;
  if (!packageName || !values.binary || !values.out) {
    throw new Error(
      `Required: --target (${Object.keys(nativeTargets).join(", ")}) --binary <prebuilt executable> --out <new directory>. See script header for usage.`,
    );
  }
  const root = fileURLToPath(new URL("../", import.meta.url));
  const metadata = JSON.parse(
    await readFile(resolve(values["package-json"] ?? join(root, "package.json")), "utf8"),
  );
  if (!metadata.version || metadata.version.constructor !== String || metadata.license !== "MIT") {
    throw new Error("The main package must declare a nonempty version and MIT license.");
  }
  const source = resolve(values.binary);
  if (!(await stat(source)).isFile()) throw new Error(`Not a binary file: ${source}`);
  const license = await readFile(resolve(values.license ?? join(root, "LICENSE")));
  // Fail before creating output if any resolved dependency's notices are missing.
  const notices = await collectNativeNotices();
  const output = resolve(values.out);
  const [os, cpu] = values.target.split("-");
  const filename = os === "win32" ? "caido-dev.exe" : "caido-dev";
  const manifest = {
    name: packageName,
    version: metadata.version,
    description: `Native caido-dev executable for ${values.target}`,
    license: "MIT",
    os: [os],
    cpu: [cpu],
    files: ["bin", "LICENSE", "THIRD_PARTY_NOTICES"],
    // npm preserves executable permissions for declared bin entries when packing.
    bin: { [`caido-dev-${values.target}`]: `bin/${filename}` },
    publishConfig: { access: "public" },
  };
  if (metadata.repository) manifest.repository = metadata.repository;
  await mkdir(dirname(output), { recursive: true });
  await mkdir(output); // Refuse existing output, including symlinks.
  await mkdir(join(output, "bin"));
  const destination = join(output, "bin", filename);
  await copyFile(source, destination);
  await chmod(destination, 0o755);
  await writeFile(join(output, "LICENSE"), license);
  await writeFile(join(output, "THIRD_PARTY_NOTICES"), notices);
  await writeFile(join(output, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  packageNative().catch((error) => {
    console.error(`package-native: ${error.message}`);
    process.exitCode = 1;
  });
}
