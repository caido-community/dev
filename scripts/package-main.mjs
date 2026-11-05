#!/usr/bin/env node
/**
 * Stage the universal npm package without changing the development manifest.
 * Usage: node scripts/package-main.mjs --out <new directory> [--source <repository>]
 * Copies dist, README.md and LICENSE, excluding the local dist/native executable.
 * Does not build, pack, install or publish. Existing output is never replaced.
 */
import { cp, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { realpathSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { z } from "zod";
import { nativeTargets } from "./native-launcher.mjs";

export async function packageMain(args = process.argv.slice(2)) {
  const { values } = parseArgs({
    args,
    options: { out: { type: "string" }, source: { type: "string" } },
    allowPositionals: false,
  });
  if (!values.out) throw new Error("Required: --out <new directory> [--source <repository>].");
  const source = resolve(values.source ?? fileURLToPath(new URL("../", import.meta.url)));
  const output = resolve(values.out);
  const manifest = z
    .object({
      name: z.literal("@caido-community/dev"),
      version: z.string().min(1),
      files: z.tuple([z.literal("dist")]),
    })
    .passthrough()
    .parse(JSON.parse(await readFile(join(source, "package.json"), "utf8")));
  const dist = join(source, "dist");
  if (!(await stat(dist)).isDirectory()) throw new Error("Expected a built dist directory.");
  for (const file of ["README.md", "LICENSE"]) {
    if (!(await stat(join(source, file))).isFile()) throw new Error(`Expected ${file}.`);
  }
  const insideDist = relative(dist, output);
  if (
    insideDist === "" ||
    (!isAbsolute(insideDist) && insideDist !== ".." && !insideDist.startsWith(`..${sep}`))
  ) {
    throw new Error("Output must be outside the source dist directory.");
  }
  manifest.optionalDependencies = Object.fromEntries(
    Object.values(nativeTargets).map((name) => [name, manifest.version]),
  );
  await mkdir(dirname(output), { recursive: true });
  await mkdir(output); // Refuse existing directories, files and symlinks.
  await cp(dist, join(output, "dist"), {
    recursive: true,
    dereference: false,
    filter: (path) => path !== join(dist, "native"),
  });
  for (const file of ["README.md", "LICENSE"]) {
    await cp(join(source, file), join(output, file));
  }
  await writeFile(join(output, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  packageMain().catch((error) => {
    console.error(`package-main: ${error.message}`);
    process.exitCode = 1;
  });
}
