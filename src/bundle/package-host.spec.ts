import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { createPackageHost } from "./package-host";
import { readAsset } from "./readme-assets";

const emptyPackage = { files: [], zip: new Uint8Array(), messages: [] };

describe("package filesystem host", () => {
  it.skipIf(process.platform === "win32")(
    "accepts absolute sources through an aliased project root but rejects leaf symlinks",
    async () => {
      const directory = await fs.mkdtemp(path.join(os.tmpdir(), "package-host-"));
      try {
        const root = path.join(directory, "project");
        const alias = path.join(directory, "alias");
        await fs.mkdir(root);
        await fs.symlink(root, alias, "dir");
        await fs.writeFile(path.join(root, "README.md"), "# Package");
        await fs.symlink(path.join(root, "README.md"), path.join(root, "linked.md"));
        const host = await createPackageHost({ cwd: alias, pluginRoots: [] });
        expect((await host.readFile(path.join(alias, "README.md"))).toString()).toBe("# Package");
        await expect(host.readFile("linked.md")).rejects.toThrow("symbolic link");
        await expect(readAsset(alias, "linked.md", "linked.md")).rejects.toThrow(
          "README asset path escapes project root",
        );
        await fs.writeFile(path.join(directory, "outside.md"), "outside");
        await expect(host.readFile("../outside.md")).rejects.toThrow("escapes project root");
      } finally {
        await fs.rm(directory, { recursive: true, force: true });
      }
    },
  );

  it.skipIf(process.platform === "win32")(
    "rejects symlinked output before reads and again before deletion",
    async () => {
      const directory = await fs.mkdtemp(path.join(os.tmpdir(), "package-output-"));
      try {
        const root = path.join(directory, "project");
        const outside = path.join(directory, "outside");
        await fs.mkdir(root);
        await fs.mkdir(outside);
        const sentinel = path.join(outside, "keep.txt");
        await fs.writeFile(sentinel, "keep");
        const host = await createPackageHost({ cwd: root, pluginRoots: [] });
        await fs.symlink(outside, path.join(root, "dist"), "dir");
        await expect(host.writePackage(emptyPackage)).rejects.toThrow(
          "Package output directory must be a real directory",
        );
        await expect(createPackageHost({ cwd: root, pluginRoots: [] })).rejects.toThrow(
          "Package output directory must be a real directory",
        );
        expect(await fs.readFile(sentinel, "utf8")).toBe("keep");
      } finally {
        await fs.rm(directory, { recursive: true, force: true });
      }
    },
  );

  it("rejects plugin roots inside output before deletion", async () => {
    const cwd = await fs.mkdtemp(path.join(os.tmpdir(), "package-plugin-root-"));
    try {
      await fs.mkdir(path.join(cwd, "dist", "plugin"), { recursive: true });
      const sentinel = path.join(cwd, "dist", "keep.txt");
      await fs.writeFile(sentinel, "keep");
      await expect(
        createPackageHost({ cwd, pluginRoots: [path.join("dist", "plugin")] }),
      ).rejects.toThrow("Plugin roots must not be inside the package output directory");
      expect(await fs.readFile(sentinel, "utf8")).toBe("keep");
    } finally {
      await fs.rm(cwd, { recursive: true, force: true });
    }
  });
});
