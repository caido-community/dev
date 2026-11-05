#!/usr/bin/env node
// Exercise only the installed launcher, adapter and dependencies, outside the checkout.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

assert.ok(process.argv[2], "Usage: node scripts/smoke-installed.mjs <installed package directory>");
const installed = resolve(process.argv[2]);
const launcher = join(installed, "dist/cli.js");
const manifest = JSON.parse(await readFile(join(installed, "package.json"), "utf8"));
const fixture = fileURLToPath(new URL("../playgrounds/build-backend/", import.meta.url));
const project = await mkdtemp(join(tmpdir(), "caido-installed-backend-"));
const env = { ...process.env };
delete env.CAIDO_DEV_BINARY;
delete env.NODE_PATH;

function run(args) {
  const result = spawnSync(process.execPath, [launcher, ...args], {
    cwd: project,
    env,
    encoding: "utf8",
    timeout: 60_000,
    maxBuffer: 4 * 1024 * 1024,
  });
  process.stdout.write(result.stdout ?? "");
  process.stderr.write(result.stderr ?? "");
  assert.ifError(result.error);
  assert.equal(
    result.status,
    0,
    `Installed CLI ${args.join(" ")} failed (${result.signal ?? result.status})`,
  );
  return result.stdout;
}

try {
  for (const file of [
    "caido.config.ts",
    "package.json",
    "README.md",
    "assets/test.png",
    "assets/test.txt",
    "packages/backend/package.json",
    "packages/backend/src/index.ts",
  ]) {
    await mkdir(dirname(join(project, file)), { recursive: true });
    await copyFile(join(fixture, file), join(project, file));
  }
  assert.match(run(["--help"]), /Usage: caido-dev/);
  assert.equal(run(["--version"]).trim(), manifest.version);
  run(["build", project]);
  const output = join(project, "dist/plugin_package");
  const packaged = JSON.parse(await readFile(join(output, "manifest.json"), "utf8"));
  assert.equal(packaged.id, "build-backend");
  assert.equal(packaged.plugins[0].entrypoint, "backend/index.js");
  assert.match(await readFile(join(output, "backend/index.js"), "utf8"), /init/);
  assert.equal(await readFile(join(output, "backend/assets/test.txt"), "utf8"), "Hello world");
  assert.match(await readFile(join(output, "README.md"), "utf8"), /Backend Plugin Playground/);
  const zip = await readFile(join(project, "dist/plugin_package.zip"));
  assert.equal(zip.readUInt32LE(0), 0x04034b50, "Expected a ZIP archive");
  console.log("Installed launcher, adapter and backend package smoke passed.");
  await rm(project, { recursive: true });
} catch (error) {
  console.error(`Installed smoke fixture retained at ${project}`);
  throw error;
}
