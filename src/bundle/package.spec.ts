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

function preloaded(relative: string[]) {
  return {
    files: [...new Set(["index.js", ...relative])].map((source) => ({
      source,
      bytes: new TextEncoder().encode(source),
    })),
    assets: [
      { pattern: "assets", files: relative.map((source) => ({ relative: source, source })) },
    ],
    readme: { content: "# Package", warnings: [], messages: [] },
  };
}

function assemble(relative: string[]) {
  return wasm.package_plugin(JSON.parse(input), preloaded(relative));
}

describe("package_plugin", () => {
  it("applies manifest defaults and validates build output plugin references", () => {
    const fixture = JSON.parse(input);
    const output = fixture.buildOutputs[0];
    const plan = (fields: typeof output) =>
      JSON.parse(wasm.plan_package(JSON.stringify({ ...fixture, buildOutputs: [fields] }))).manifest
        .plugins[0];

    for (const kind of ["frontend", "backend"]) {
      const fields = { ...output, kind };
      expect(plan(fields).name).toBe("backend");
      expect(plan({ ...fields, name: null }).name).toBe("backend");
      expect(plan({ ...fields, name: "" }).name).toBe("");
    }
    const frontend = plan({ ...output, kind: "frontend", cssFileName: "", backendId: "" });
    expect(frontend).not.toHaveProperty("style");
    expect(frontend.backend).toBeNull();
    expect(plan(output)).not.toHaveProperty("style");
    expect(() => plan({ ...output, backendId: "../escape" })).toThrow("Invalid plugin id");
    expect(() => plan({ ...output, backend: { id: "../escape" } })).toThrow("Invalid plugin id");
  });

  it("preserves supplied manifest fields and order while validating referenced ids", () => {
    const fixture = JSON.parse(input);
    const manifest = {
      extension: { nested: true },
      plugins: [{ extra: "retained", id: "supplied", backendId: "" }],
      id: "package",
      name: "Supplied",
    };
    const plan = (config = fixture.config, supplied = manifest) =>
      JSON.parse(
        wasm.plan_package(JSON.stringify({ config, manifest: supplied, buildOutputs: [] })),
      ).manifest;
    expect(JSON.stringify(plan())).toBe(JSON.stringify(manifest));
    expect(() =>
      plan({ ...fixture.config, plugins: [{ id: "configured", backendId: "../escape" }] }),
    ).toThrow("Invalid plugin id");
    expect(() =>
      plan(fixture.config, { ...manifest, plugins: [{ ...manifest.plugins[0], id: "../escape" }] }),
    ).toThrow("Invalid plugin id");
    expect(() =>
      plan(fixture.config, {
        ...manifest,
        plugins: [{ ...manifest.plugins[0], backendId: "../escape" }],
      }),
    ).toThrow("Invalid plugin id");
  });

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
      await fs.writeFile(path.join(cwd, "README.md"), "![missing](missing.png)");
      await expect(bundlePackage({ cwd, config, buildOutputs: [] })).rejects.toThrow();
      expect(await fs.readFile(sentinel, "utf8")).toBe("keep");
      await fs.writeFile(path.join(cwd, "README.md"), "# Package");
      await expect(
        bundlePackage({
          cwd,
          config,
          buildOutputs: [
            { kind: "backend", id: "backend", name: "Backend", fileName: "missing.js", assets: [] },
          ],
        }),
      ).rejects.toThrow();
      expect(await fs.readFile(sentinel, "utf8")).toBe("keep");
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
    ["/absolute.txt"],
  ])("rejects unsafe or colliding asset destinations: %j", async (...relative) => {
    expect(() => assemble(relative)).toThrow();
  });

  it("packages shared sources and patterns for separate plugin destinations", async () => {
    const cwd = await fs.mkdtemp(path.join(os.tmpdir(), "package-shared-"));
    try {
      await fs.mkdir(path.join(cwd, "assets"));
      await fs.writeFile(path.join(cwd, "assets", "shared.txt"), "shared asset");
      await fs.writeFile(path.join(cwd, "README.md"), "# Shared");
      await fs.writeFile(path.join(cwd, "index.js"), "export {};");
      await bundlePackage({
        cwd,
        config: caidoConfigSchema.parse(JSON.parse(input).config),
        buildOutputs: ["first", "second"].map((id) => ({
          kind: "backend",
          id,
          name: id,
          fileName: "index.js",
          assets: ["assets"],
        })),
      });
      const zip = await JSZip.loadAsync(
        await fs.readFile(path.join(cwd, "dist", "plugin_package.zip")),
      );
      for (const id of ["first", "second"]) {
        expect(await zip.file(`${id}/index.js`)?.async("string")).toBe("export {};");
        expect(await zip.file(`${id}/assets/shared.txt`)?.async("string")).toBe("shared asset");
      }
    } finally {
      await fs.rm(cwd, { recursive: true, force: true });
    }
  });
});
