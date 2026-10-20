import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import JSZip from "jszip";
import { describe, expect, it } from "vitest";

import { caidoConfigSchema } from "../types";
import { wasm } from "../wasm";

import { bundlePackage } from "./index";

const input = JSON.stringify({
  config: {
    id: "package",
    name: "Package",
    description: "Package fixture",
    version: "1.0.0",
    author: { name: "Author" },
    plugins: [],
  },
  buildOutputs: [
    {
      id: "backend",
      kind: "backend",
      fileName: "index.js",
      assets: ["assets"],
    },
  ],
});

function assemble(relative: string[]) {
  return wasm.package_plugin(
    input,
    (source) =>
      Promise.resolve(new TextEncoder().encode(source === "README.md" ? "# Package" : source)),
    () => Promise.resolve(relative.map((name) => ({ relative: name, source: name }))),
    () => Promise.reject(new Error("Unexpected README asset")),
    () => Promise.reject(new Error("Unexpected encoding")),
    () => true,
  );
}

describe("package_plugin", () => {
  it("leaves existing outputs intact for missing README and invalid inputs", async () => {
    const cwd = await fs.mkdtemp(path.join(os.tmpdir(), "package-test-"));
    try {
      await fs.mkdir(path.join(cwd, "dist"));
      const sentinel = path.join(cwd, "dist", "keep.txt");
      await fs.writeFile(sentinel, "keep");
      const config = caidoConfigSchema.parse(JSON.parse(input).config);
      await expect(bundlePackage({ cwd, config, buildOutputs: [] })).rejects.toThrow(
        "README.md is required",
      );
      expect(await fs.readFile(sentinel, "utf8")).toBe("keep");
      await fs.writeFile(path.join(cwd, "README.md"), "# Package");
      await expect(
        bundlePackage({
          cwd,
          config: { ...config, id: "../escape" },
          buildOutputs: [],
        }),
      ).rejects.toBeTruthy();
      expect(await fs.readFile(sentinel, "utf8")).toBe("keep");
    } finally {
      await fs.rm(cwd, { recursive: true, force: true });
    }
  });
  it.each([false, true])(
    "handles recursive asset matches while rejecting distinct-source collisions: %s",
    async (collision) => {
      const cwd = await fs.mkdtemp(path.join(os.tmpdir(), "package-assets-"));
      try {
        await fs.mkdir(path.join(cwd, "assets", "data"), { recursive: true });
        await fs.writeFile(path.join(cwd, "assets", "data", "data.txt"), "nested asset");
        if (collision) {
          await fs.writeFile(path.join(cwd, "assets", "data.txt"), "different asset");
        }
        await fs.writeFile(path.join(cwd, "README.md"), "# Package");
        await fs.writeFile(path.join(cwd, "index.js"), "export {};");
        const packaging = bundlePackage({
          cwd,
          config: caidoConfigSchema.parse(JSON.parse(input).config),
          buildOutputs: [
            {
              id: "backend",
              kind: "backend",
              name: "Backend",
              fileName: "index.js",
              assets: ["assets/**/*"],
            },
          ],
        });
        if (collision) {
          await expect(packaging).rejects.toThrow("Duplicate package destination");
          await expect(fs.access(path.join(cwd, "dist"))).rejects.toThrow();
        } else {
          await packaging;
          const zip = await JSZip.loadAsync(
            await fs.readFile(path.join(cwd, "dist", "plugin_package.zip")),
          );
          expect(await zip.file("backend/assets/data.txt")?.async("string")).toBe("nested asset");
          expect(
            await fs.readFile(
              path.join(cwd, "dist", "plugin_package", "backend", "assets", "data.txt"),
              "utf8",
            ),
          ).toBe("nested asset");
        }
      } finally {
        await fs.rm(cwd, { recursive: true, force: true });
      }
    },
  );

  it.each([
    ["same.txt", "same.txt"],
    ["same.txt", "SAME.txt"],
    ["folder", "folder/file.txt"],
    ["folder/file.txt", "folder"],
    ["Folder", "folder/file.txt"],
    ["Folder/file.txt", "folder"],
    ["../escape.txt"],
    [""],
    ["/absolute.txt"],
    ["."],
    [".."],
    ["./file.txt"],
    ["nested/./file.txt"],
    ["nested/../file.txt"],
    ["nested//file.txt"],
    ["nested/file.txt/"],
    ["nested\\file.txt"],
    ["nested/CON"],
  ])("rejects unsafe or colliding asset destinations: %j", async (...relative) => {
    await expect(assemble(relative)).rejects.toBeTruthy();
  });

  it("returns actual byte arrays matching the archive", async () => {
    const result = await assemble(["nested/file.txt"]);
    expect(result.zip).toBeInstanceOf(Uint8Array);
    const zip = await JSZip.loadAsync(result.zip);
    for (const file of result.files) {
      expect(file.bytes).toBeInstanceOf(Uint8Array);
      expect(await zip.file(file.path)?.async("uint8array")).toEqual(file.bytes);
    }
    expect(result.files.map((file) => file.path)).toContain("backend/assets/nested/file.txt");
  });
});
