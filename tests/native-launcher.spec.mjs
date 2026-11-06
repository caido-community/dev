import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, onTestFinished, test } from "vitest";
import { resolveNativeBinary } from "../scripts/native-launcher.mjs";

const launcher = fileURLToPath(new URL("../scripts/native-launcher.mjs", import.meta.url));

async function temporary() {
  const directory = await realpath(await mkdtemp(join(tmpdir(), "caido launcher ")));
  onTestFinished(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

describe("native launcher", () => {
  test("unsupported and omitted optional dependencies have actionable errors", () => {
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

  test("only an explicit override bypasses platform resolution", () => {
    assert.equal(
      resolveNativeBinary({ platform: "unknown", env: { CAIDO_DEV_BINARY: process.execPath } }),
      process.execPath,
    );
  });

  test("failed executable startup gives a nonzero actionable diagnostic", async () => {
    const directory = await temporary();
    const result = spawnSync(process.execPath, [launcher], {
      encoding: "utf8",
      env: { ...process.env, CAIDO_DEV_BINARY: join(directory, "missing") },
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Cannot start native executable.*CAIDO_DEV_BINARY/);
  });

  for (const signal of ["SIGINT", "SIGTERM"]) {
    test(
      `forwards ${signal} and preserves child termination signal`,
      { skip: process.platform === "win32", timeout: 10000 },
      async () => {
        const directory = await temporary();
        const fixture = join(directory, "signal.cjs");
        await writeFile(fixture, "console.log('ready');setInterval(()=>{},1000);");
        const child = spawn(process.execPath, [launcher, fixture], {
          env: { ...process.env, CAIDO_DEV_BINARY: process.execPath },
          stdio: ["ignore", "pipe", "pipe"],
        });
        onTestFinished(() => {
          child.kill("SIGKILL");
        });
        const closed = once(child, "close");
        await once(child.stdout, "data");
        child.kill(signal);
        const [code, observedSignal] = await closed;
        assert.equal(code, null);
        assert.equal(observedSignal, signal);
      },
    );
  }
});
