import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { packageMain } from "./package-main.mjs";

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), "caido-main-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const source = join(root, "source");
  const output = join(root, "stage");
  await mkdir(join(source, "dist", "native"), { recursive: true });
  const manifest = {
    name: "@caido-community/dev",
    version: "1.2.3-beta.4",
    type: "module",
    files: ["dist"],
    bin: { "caido-dev": "./dist/cli.js" },
    exports: { ".": { types: "./dist/index.d.ts", default: "./dist/index.js" } },
    engines: { node: ">=20", pnpm: ">=9" },
    dependencies: { jiti: "2.4.2" },
    optionalDependencies: { obsolete: "0.0.0" },
  };
  await writeFile(join(source, "package.json"), JSON.stringify(manifest));
  for (const file of [
    "dist/cli.js",
    "dist/index.js",
    "dist/index.d.ts",
    "dist/native/caido-dev",
    "README.md",
    "LICENSE",
  ]) {
    await writeFile(join(source, file), `fixture ${file}`);
  }
  return { source, output, manifest };
}

void test("stages exact native dependencies and preserves universal package metadata and files", async (t) => {
  const { source, output, manifest } = await fixture(t);
  const original = await readFile(join(source, "package.json"), "utf8");
  const result = await packageMain(["--source", source, "--out", output]);
  const staged = JSON.parse(await readFile(join(output, "package.json"), "utf8"));
  const optionalDependencies = Object.fromEntries(
    ["linux-x64", "linux-arm64", "win32-x64", "darwin-arm64"].map((target) => [
      `@caido-community/dev-${target}`,
      manifest.version,
    ]),
  );
  assert.deepEqual(staged, { ...manifest, optionalDependencies });
  assert.deepEqual(result, staged);
  assert.equal(await readFile(join(source, "package.json"), "utf8"), original);
  assert.deepEqual((await readdir(output)).sort(), [
    "LICENSE",
    "README.md",
    "dist",
    "package.json",
  ]);
  assert.deepEqual((await readdir(join(output, "dist"))).sort(), [
    "cli.js",
    "index.d.ts",
    "index.js",
  ]);
  assert.equal(await readFile(join(output, "README.md"), "utf8"), "fixture README.md");
  assert.equal(await readFile(join(output, "LICENSE"), "utf8"), "fixture LICENSE");
  assert.equal(
    await readFile(join(source, "dist/native/caido-dev"), "utf8"),
    "fixture dist/native/caido-dev",
  );
  await assert.rejects(stat(join(output, "dist/native")), { code: "ENOENT" });
});

void test("refuses existing staging output without changing it", async (t) => {
  const { source, output } = await fixture(t);
  await mkdir(output);
  await writeFile(join(output, "sentinel"), "keep");
  await assert.rejects(packageMain(["--source", source, "--out", output]), { code: "EEXIST" });
  assert.deepEqual(await readdir(output), ["sentinel"]);
  assert.equal(await readFile(join(output, "sentinel"), "utf8"), "keep");
});

void test("rejects invalid versions and changed published file lists before creating output", async (t) => {
  const { source, output, manifest } = await fixture(t);
  for (const patch of [{ version: "" }, { version: 123 }, { files: ["dist", "extra"] }]) {
    await writeFile(join(source, "package.json"), JSON.stringify({ ...manifest, ...patch }));
    await assert.rejects(packageMain(["--source", source, "--out", output]));
    await assert.rejects(stat(output), { code: "ENOENT" });
  }
});

void test("rejects missing build output and recursive staging destinations", async (t) => {
  const { source, output } = await fixture(t);
  await assert.rejects(
    packageMain(["--source", source, "--out", join(source, "dist/stage")]),
    /outside/,
  );
  await rm(join(source, "dist"), { recursive: true });
  await assert.rejects(packageMain(["--source", source, "--out", output]), { code: "ENOENT" });
  await assert.rejects(stat(output), { code: "ENOENT" });
});

void test("requires an explicit output and rejects unknown arguments", async () => {
  await assert.rejects(packageMain([]), /--out/);
  await assert.rejects(packageMain(["--unknown"]), /Unknown option/);
});
