import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { nativeTargets, resolveNativeBinary } from "./native-launcher.mjs";
import { packageNative } from "./package-native.mjs";

const launcher = fileURLToPath(new URL("./native-launcher.mjs", import.meta.url));

async function temporary(t) {
  const directory = await realpath(await mkdtemp(join(tmpdir(), "caido launcher ")));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

void test("selects exactly the four optional packages and Windows executable name", () => {
  for (const [target, name] of Object.entries(nativeTargets)) {
    const [platform, arch] = target.split("-");
    const result = resolveNativeBinary({
      platform,
      arch,
      env: {},
      resolveModule: (value) => value,
    });
    assert.equal(result, `${name}/bin/caido-dev${platform === "win32" ? ".exe" : ""}`);
  }
});

void test("unsupported and omitted optional dependencies have actionable errors", () => {
  assert.throws(
    () => resolveNativeBinary({ platform: "darwin", arch: "x64", env: {} }),
    /Unsupported platform darwin-x64.*Supported targets/,
  );
  assert.throws(
    () =>
      resolveNativeBinary({
        platform: "linux",
        arch: "x64",
        env: {},
        resolveModule() {
          throw new Error("missing");
        },
      }),
    /dev-linux-x64.*optional dependencies enabled/,
  );
});

void test("only an explicit override bypasses platform resolution", () => {
  assert.equal(
    resolveNativeBinary({ platform: "unknown", env: { CAIDO_DEV_BINARY: process.execPath } }),
    process.execPath,
  );
});

void test("optional package binary subpaths resolve from the installed launcher", async (t) => {
  const directory = await temporary(t);
  const installed = join(directory, "dist", "native-launcher.mjs");
  await mkdir(dirname(installed), { recursive: true });
  await copyFile(launcher, installed);
  for (const [target, name] of Object.entries(nativeTargets)) {
    const [platform, arch] = target.split("-");
    const binary = join(
      directory,
      "node_modules",
      name,
      "bin",
      `caido-dev${platform === "win32" ? ".exe" : ""}`,
    );
    await mkdir(dirname(binary), { recursive: true });
    await writeFile(binary, "fixture");
    assert.equal(
      resolveNativeBinary({
        platform,
        arch,
        env: {},
        resolveModule: createRequire(installed).resolve,
      }),
      binary,
    );
  }
});

void test("launcher preserves arguments, cwd, stdio, exit code, and adapter environment", async (t) => {
  const directory = await temporary(t);
  const installed = join(directory, "dist", "native-launcher.mjs");
  await mkdir(dirname(installed), { recursive: true });
  await copyFile(launcher, installed);
  const fixture = join(directory, "child.cjs");
  await writeFile(
    fixture,
    `console.log(JSON.stringify({args:process.argv.slice(2),cwd:process.cwd(),node:process.env.CAIDO_DEV_NODE,adapter:process.env.CAIDO_DEV_ADAPTER}));console.error('child stderr');process.exitCode=2;`,
  );
  const result = spawnSync(process.execPath, [installed, fixture, "a b", ";not shell"], {
    cwd: directory,
    encoding: "utf8",
    env: {
      ...process.env,
      CAIDO_DEV_BINARY: process.execPath,
      CAIDO_DEV_NODE: "wrong",
      CAIDO_DEV_ADAPTER: "wrong",
    },
  });
  assert.equal(result.status, 2);
  assert.equal(result.stderr.trim(), "child stderr");
  assert.deepEqual(JSON.parse(result.stdout), {
    args: ["a b", ";not shell"],
    cwd: directory,
    node: process.execPath,
    adapter: join(directory, "dist", "adapter.js"),
  });
});

void test("failed executable startup gives a nonzero actionable diagnostic", async (t) => {
  const directory = await temporary(t);
  const result = spawnSync(process.execPath, [launcher], {
    encoding: "utf8",
    env: { ...process.env, CAIDO_DEV_BINARY: join(directory, "missing") },
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Cannot start native executable.*CAIDO_DEV_BINARY/);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  void test(
    `forwards ${signal} and preserves child termination signal`,
    { skip: process.platform === "win32", timeout: 10000 },
    async (t) => {
      const directory = await temporary(t);
      const fixture = join(directory, "signal.cjs");
      await writeFile(fixture, "console.log('ready');setInterval(()=>{},1000);");
      const child = spawn(process.execPath, [launcher, fixture], {
        env: { ...process.env, CAIDO_DEV_BINARY: process.execPath },
        stdio: ["ignore", "pipe", "pipe"],
      });
      t.after(() => child.kill("SIGKILL"));
      const closed = once(child, "close");
      await once(child.stdout, "data");
      child.kill(signal);
      const [code, observedSignal] = await closed;
      assert.equal(code, null);
      assert.equal(observedSignal, signal);
    },
  );
}

void test("generates all platform packages with matching version, license, and executable", async (t) => {
  const directory = await temporary(t);
  const source = join(directory, "prebuilt");
  const metadata = join(directory, "main.json");
  const license = join(directory, "MIT.txt");
  await writeFile(source, "prebuilt bytes");
  await writeFile(metadata, JSON.stringify({ version: "1.2.3-beta.1", license: "MIT" }));
  await writeFile(license, "MIT license fixture");
  for (const [target, name] of Object.entries(nativeTargets)) {
    const out = join(directory, target);
    const args = [
      "--target",
      target,
      "--binary",
      source,
      "--out",
      out,
      "--package-json",
      metadata,
      "--license",
      license,
    ];
    await packageNative(args);
    const manifest = JSON.parse(await readFile(join(out, "package.json"), "utf8"));
    const [os, cpu] = target.split("-");
    assert.equal(manifest.name, name);
    assert.equal(manifest.version, "1.2.3-beta.1");
    assert.equal(manifest.license, "MIT");
    assert.deepEqual(manifest.os, [os]);
    assert.deepEqual(manifest.cpu, [cpu]);
    const binary = join(out, "bin", `caido-dev${os === "win32" ? ".exe" : ""}`);
    assert.equal(await readFile(binary, "utf8"), "prebuilt bytes");
    assert.equal(await readFile(join(out, "LICENSE"), "utf8"), "MIT license fixture");
    if (process.platform !== "win32") assert.equal((await stat(binary)).mode & 0o777, 0o755);
    await assert.rejects(packageNative(args), /EEXIST/);
  }
  await assert.rejects(
    packageNative([
      "--target",
      "darwin-x64",
      "--binary",
      source,
      "--out",
      join(directory, "invalid"),
    ]),
    /Required: --target/,
  );
});
