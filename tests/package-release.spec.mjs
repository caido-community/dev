import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rm, stat, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, onTestFinished, test } from "vitest";
import { packageMain, packageNative, packageRelease } from "../scripts/package-release.mjs";
import { nativeTargets } from "../scripts/native-launcher.mjs";

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "caido-main-package-"));
  onTestFinished(() => rm(root, { recursive: true, force: true }));
  const source = join(root, "source");
  const output = join(root, "stage");
  await mkdir(join(source, "dist", "native"), { recursive: true });
  const manifest = {
    name: "@caido-community/dev",
    version: "1.2.3-beta.4",
    license: "MIT",
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
    "dist/adapter.js",
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

describe("native package staging", () => {
  test("generates all platform packages with matching version, license, and executable", async () => {
    const { source, output, manifest } = await fixture();
    const binarySource = join(source, "dist/native/caido-dev");
    for (const [target, name] of Object.entries(nativeTargets)) {
      const out = join(output, target);
      const options = { target, binary: binarySource, source, out };
      await packageNative(options);
      const staged = JSON.parse(await readFile(join(out, "package.json"), "utf8"));
      const [os, cpu] = target.split("-");
      assert.equal(staged.name, name);
      assert.equal(staged.version, manifest.version);
      assert.equal(staged.license, manifest.license);
      assert.deepEqual(staged.os, [os]);
      assert.deepEqual(staged.cpu, [cpu]);
      const binary = join(out, "bin", `caido-dev${os === "win32" ? ".exe" : ""}`);
      assert.equal(await readFile(binary, "utf8"), "fixture dist/native/caido-dev");
      assert.equal(await readFile(join(out, "LICENSE"), "utf8"), "fixture LICENSE");
      if (process.platform !== "win32") assert.equal((await stat(binary)).mode & 0o777, 0o755);
      await assert.rejects(packageNative(options), /EEXIST/);
    }
    await assert.rejects(
      packageNative({
        source,
        target: "darwin-x64",
        binary: binarySource,
        out: join(output, "invalid"),
      }),
      /Required: target/,
    );
  });
});

describe("main package staging", () => {
  test("stages exact native dependencies and preserves universal package metadata and files", async () => {
    const { source, output, manifest } = await fixture();
    const original = await readFile(join(source, "package.json"), "utf8");
    const result = await packageMain({ source, out: output });
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
      "adapter.js",
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

  test("refuses existing staging output without changing it", async () => {
    const { source, output } = await fixture();
    await mkdir(output);
    await writeFile(join(output, "sentinel"), "keep");
    await assert.rejects(packageMain({ source, out: output }), { code: "EEXIST" });
    assert.deepEqual(await readdir(output), ["sentinel"]);
    assert.equal(await readFile(join(output, "sentinel"), "utf8"), "keep");
  });

  test("rejects invalid versions and changed published file lists before creating output", async () => {
    const { source, output, manifest } = await fixture();
    for (const patch of [{ version: "" }, { version: 123 }, { files: ["dist", "extra"] }]) {
      await writeFile(join(source, "package.json"), JSON.stringify({ ...manifest, ...patch }));
      await assert.rejects(packageMain({ source, out: output }));
      await assert.rejects(stat(output), { code: "ENOENT" });
    }
  });

  test("rejects missing build output and recursive staging destinations", async () => {
    const { source, output } = await fixture();
    await assert.rejects(packageMain({ source, out: join(source, "dist/stage") }), /outside/);
    await rm(join(source, "dist"), { recursive: true });
    await assert.rejects(packageMain({ source, out: output }), { code: "ENOENT" });
    await assert.rejects(stat(output), { code: "ENOENT" });
  });

  test("copies only distribution files and leaves junk source outputs untouched", async () => {
    const { source, output } = await fixture();
    await mkdir(join(source, "dist/community-audit/node_modules"), { recursive: true });
    await writeFile(join(source, "dist/community-audit/node_modules/.modules.yaml"), "keep");
    await writeFile(join(source, "dist/stale.js"), "keep");
    for (const file of ["index.js.map", "adapter.js.map", "adapter.d.ts"]) {
      await writeFile(join(source, "dist", file), file);
    }
    await packageMain({ source, out: output });
    assert.deepEqual((await readdir(join(output, "dist"))).sort(), [
      "adapter.d.ts",
      "adapter.js",
      "adapter.js.map",
      "cli.js",
      "index.d.ts",
      "index.js",
      "index.js.map",
    ]);
    assert.equal(
      await readFile(join(source, "dist/community-audit/node_modules/.modules.yaml"), "utf8"),
      "keep",
    );
    assert.equal(await readFile(join(source, "dist/stale.js"), "utf8"), "keep");
  });

  test("rejects a missing required adapter before creating output", async () => {
    const { source, output } = await fixture();
    await rm(join(source, "dist/adapter.js"));
    await assert.rejects(packageMain({ source, out: output }), { code: "ENOENT" });
    await assert.rejects(stat(output), { code: "ENOENT" });
  });

  test.skipIf(process.platform === "win32")(
    "rejects allowlisted symlinks instead of copying their contents",
    async () => {
      const { source, output } = await fixture();
      await symlink(join(source, "README.md"), join(source, "dist/index.js.map"));
      await assert.rejects(packageMain({ source, out: output }), /regular dist\/index.js.map/);
      await assert.rejects(stat(output), { code: "ENOENT" });
    },
  );

  test("unified command stages both packages with one source version", async () => {
    const { source, output, manifest } = await fixture();
    const result = await packageRelease([
      "--source",
      source,
      "--target",
      "linux-x64",
      "--binary",
      process.execPath,
      "--out",
      output,
    ]);
    assert.deepEqual((await readdir(output)).sort(), ["main", "native"]);
    for (const name of ["main", "native"]) {
      const packed = JSON.parse(await readFile(join(output, name, "package.json"), "utf8"));
      assert.equal(packed.version, manifest.version);
      assert.deepEqual(result[name], packed);
    }
    assert.equal(result.main.optionalDependencies[result.native.name], result.native.version);
    await assert.rejects(
      packageRelease([
        "--source",
        source,
        "--target",
        "linux-x64",
        "--binary",
        process.execPath,
        "--out",
        output,
      ]),
      { code: "EEXIST" },
    );
  });

  test("requires an explicit output and rejects unknown arguments", async () => {
    await assert.rejects(packageMain(), /out/);
    await assert.rejects(packageRelease([]), /--out/);
    await assert.rejects(packageRelease(["--unknown"]), /Unknown option/);
  });
});
