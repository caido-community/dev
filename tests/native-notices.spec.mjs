import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { describe, onTestFinished, test } from "vitest";
import { collectNativeNotices, nativeCargoMetadata } from "../scripts/native-notices.mjs";
import { packageNative } from "../scripts/package-release.mjs";

const metadata = await nativeCargoMetadata();
const notices = await collectNativeNotices(metadata);

function dependency(name, version) {
  const pkg = metadata.packages.find((item) => item.name === name && item.version === version);
  assert.ok(pkg, `resolved ${name}@${version}`);
  return pkg;
}

describe("native dependency notices", () => {
  test("copies authentic audited WebP, CC0 and ISC materials verbatim", async () => {
    const audited = [
      ["webp", "0.3.1", ["LICENSE-MIT", "LICENSE-APACHE"]],
      ["libwebp-sys", "0.9.6", ["vendor/COPYING", "vendor/PATENTS", "vendor/AUTHORS"]],
      ["notify", "8.2.0", ["LICENSE-CC0"]],
      ["inotify", "0.11.5", ["LICENSE"]],
      ["inotify-sys", "0.1.8", ["LICENSE"]],
    ];
    for (const [name, version, files] of audited) {
      for (const file of files) {
        const text = await readFile(
          join(dirname(dependency(name, version).manifest_path), file),
          "utf8",
        );
        assert.ok(notices.includes(text), `${name}@${version}/${file} copied in full`);
      }
    }
    assert.match(notices, /Jared Forth/);
    assert.match(notices, /Hanno Braun/);
    assert.match(notices, /Google Inc/);
  });

  test("covers every non-workspace resolved dependency, including non-host targets", () => {
    for (const pkg of metadata.packages) {
      if (!metadata.workspace_members.includes(pkg.id)) {
        assert.ok(notices.includes(`\n${pkg.name}@${pkg.version}\n`));
      }
    }
    assert.match(notices, /windows-sys@/);
    assert.match(notices, /inotify@0.11.5/);
    assert.match(notices, /zune-jpeg@0.4.21/);
    assert.match(notices, /source header/);
    assert.match(notices, /Apache-2.0 license text \(from webp@0.3.1\/LICENSE-APACHE\)/);
  });

  test("missing required WebP files fail rather than fall back to license metadata", async () => {
    const directory = await mkdtemp(join(tmpdir(), "caido missing notices "));
    onTestFinished(() => rm(directory, { recursive: true, force: true }));
    const pkg = dependency("libwebp-sys", "0.9.6");
    await assert.rejects(
      collectNativeNotices({
        ...metadata,
        packages: [{ ...pkg, manifest_path: join(directory, "Cargo.toml") }],
      }),
      /Missing required notice for libwebp-sys@0.9.6: vendor\/COPYING/,
    );
  });

  test("unrecognized dependency with no license text fails closed", async () => {
    const directory = await mkdtemp(join(tmpdir(), "caido absent license "));
    onTestFinished(() => rm(directory, { recursive: true, force: true }));
    await assert.rejects(
      collectNativeNotices({
        ...metadata,
        packages: [
          {
            id: "test",
            name: "test",
            version: "1",
            license: "MIT",
            manifest_path: join(directory, "Cargo.toml"),
          },
        ],
      }),
      /No license text found for test@1/,
    );
  });

  test("all four native package manifests ship the complete notices file", async () => {
    const directory = await mkdtemp(join(tmpdir(), "caido packaged notices "));
    onTestFinished(() => rm(directory, { recursive: true, force: true }));
    for (const target of ["linux-x64", "linux-arm64", "win32-x64", "darwin-arm64"]) {
      const output = join(directory, target);
      const manifest = await packageNative({ target, binary: process.execPath, out: output });
      assert.ok(manifest.files.includes("THIRD_PARTY_NOTICES"));
      assert.deepEqual(manifest.bin, {
        [`caido-dev-${target}`]: `bin/caido-dev${target === "win32-x64" ? ".exe" : ""}`,
      });
      assert.equal(await readFile(join(output, "THIRD_PARTY_NOTICES"), "utf8"), notices);
    }
  });
});
