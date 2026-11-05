import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { Glob } from "glob";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

const adapter = path.resolve("dist/adapter.js");
let directory: string;

type AdapterRequestInput = {
  version?: number;
  operation?: string;
  cwd?: string | number;
  config?: string | boolean;
  expectedConfig?: unknown;
  extra?: boolean;
};

function invoke(request: AdapterRequestInput, cwd = directory) {
  return spawnSync(process.execPath, [adapter], {
    cwd,
    input: JSON.stringify(request),
    encoding: "utf8",
    timeout: 30_000,
  });
}

function invokeRaw(input: string, cwd = directory) {
  return spawnSync(process.execPath, [adapter], {
    cwd,
    input,
    encoding: "utf8",
    timeout: 30_000,
  });
}

function request(operation: "load" | "build" = "load") {
  return { version: 1, operation, cwd: directory };
}

const config = `{
  id: "test", name: "Test", description: "Adapter test", version: "1.0.0",
  author: { name: "Tester" }, watch: { port: 3210 }, plugins: []
}`;

beforeEach(async () => {
  directory = await mkdtemp(path.join(os.tmpdir(), "caido-adapter-"));
  await writeFile(path.join(directory, "caido.config.ts"), `export default ${config}`);
});

afterEach(async () => {
  await rm(directory, { recursive: true, force: true });
});

describe("one-shot Node adapter", () => {
  it("loads imported TS config, keeping console and direct stdout off the protocol", async () => {
    await writeFile(path.join(directory, "metadata.ts"), `export default ${config}`);
    await writeFile(
      path.join(directory, "custom.ts"),
      `
      import config from "./metadata";
      console.log("config console output");
      process.stdout.write("config direct output\\n");
      export default config;
    `,
    );
    const result = invoke({ ...request(), config: "./custom.ts" });
    expect(result.error).toBeUndefined();
    expect(result.status).toBe(0);
    expect(result.stderr).toContain("config console output");
    expect(result.stderr).toContain("config direct output");
    expect(JSON.parse(result.stdout)).toEqual({
      version: 1,
      config: JSON.parse(config.replace(/(\w+):/g, '"$1":')),
      buildOutputs: [],
      assetMatches: {},
    });
    expect(result.stdout.trim().split("\n")).toHaveLength(1);
  });

  it("uses the requested project root without changing the process cwd", async () => {
    const result = invoke(request(), path.dirname(directory));
    expect(result.status).toBe(0);
    expect(JSON.parse(result.stdout).config.id).toBe("test");
  });

  it("rejects malformed JSON without emitting a response", () => {
    const result = invokeRaw("{");
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr.length).toBeGreaterThan(0);
  });

  it.each([
    { version: 2, operation: "load", cwd: "test" },
    { version: 1, operation: "watch", cwd: "test" },
    { version: 1, operation: "load", cwd: 12 },
    { version: 1, operation: "load", cwd: "test", config: false },
    { version: 1, operation: "load", cwd: "test", extra: true },
  ])("rejects malformed requests without emitting a response: %j", (input) => {
    const result = invoke(input);
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr.length).toBeGreaterThan(0);
  });

  it("rejects changed metadata before invoking the build", async () => {
    const loaded = invoke(request());
    expect(loaded.status).toBe(0);
    const expectedConfig = JSON.parse(loaded.stdout).config;
    expect(invoke({ ...request("build"), expectedConfig }).status).toBe(0);
    await writeFile(
      path.join(directory, "caido.config.ts"),
      `export default { ...${config}, id: "changed" };`,
    );
    const result = invoke({ ...request("build"), expectedConfig });
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain("Configuration metadata changed after native preflight");
  });

  it("reports config schema errors on stderr", async () => {
    await writeFile(path.join(directory, "caido.config.ts"), `export default { invalid: true };`);
    const result = invoke(request());
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain("Failed to load configuration");
  });

  it("preserves executable Vite hooks for builds but returns metadata only", async () => {
    await writeFile(path.join(directory, "entry.ts"), `export const value = 42;`);
    await writeFile(
      path.join(directory, "caido.config.ts"),
      `
      export default { ...${config}, plugins: [{
        id: "ui", kind: "frontend", root: ".", assets: ["assets/**"], backend: null,
        vite: {
          build: { lib: { entry: "entry.ts" } },
          plugins: [{ name: "adapter-test", buildStart() {
            console.log("vite hook ran");
            process.stdout.write("vite direct output\\n");
          }}]
        }
      }] };
    `,
    );
    const loaded = invoke(request());
    expect(loaded.status).toBe(0);
    expect(JSON.parse(loaded.stdout).config.plugins).toEqual([
      { id: "ui", kind: "frontend", root: ".", assets: ["assets/**"], backend: null },
    ]);
    const built = invoke(request("build"));
    expect(built.error).toBeUndefined();
    expect(built.status, built.stderr).toBe(0);
    expect(built.stderr).toContain("vite hook ran");
    expect(built.stderr).toContain("vite direct output");
    const response = JSON.parse(built.stdout);
    expect(response.config).toEqual(JSON.parse(loaded.stdout).config);
    expect(response.assetMatches).toEqual({ "assets/**": [] });
    expect(JSON.parse(loaded.stdout).assetMatches).toEqual({});
    expect(response.buildOutputs).toMatchObject([{ kind: "frontend", id: "ui" }]);
    expect(await readFile(response.buildOutputs[0].fileName, "utf8")).toContain("42");
  });

  it("returns legacy glob matches only, after builds, with distinct pattern keys", async () => {
    const files = [
      "assets/a.txt",
      "assets/b.txt",
      "assets/c.json",
      "assets/.hidden.txt",
      "assets/literal[1].txt",
      "assets/nested/deep.txt",
      "assets/nested/.hidden.txt",
      "assets/other/deep.json",
      "__proto__",
    ];
    for (const file of files) {
      await mkdir(path.dirname(path.join(directory, file)), { recursive: true });
      await writeFile(path.join(directory, file), "asset contents remain in Rust");
    }
    const patterns = [
      "assets/{a,b}.txt",
      "assets/{nested,other}/*.{txt,json}",
      "assets/{a,{b,c}}.*",
      "assets/[a-b].txt",
      "assets/@(a|b).txt",
      "assets/!(a).txt",
      "assets/literal\\[1\\].txt",
      "assets/*",
      "assets/**",
      "assets/.*",
      "assets/nested",
      "generated.txt",
      "missing/**",
      "__proto__",
    ];
    await writeFile(path.join(directory, "entry.ts"), "export const value = 42;");
    await writeFile(
      path.join(directory, "caido.config.ts"),
      `
      import { writeFileSync } from "node:fs";
      export default { ...${config}, plugins: [{
        id: "ui", kind: "frontend", root: ".",
        assets: ${JSON.stringify([...patterns, patterns[0]])},
        vite: {
          build: { lib: { entry: "entry.ts" } },
          plugins: [{ name: "create-asset", closeBundle() {
            writeFileSync(${JSON.stringify(path.join(directory, "generated.txt"))}, "generated");
          }}]
        }
      }] };
    `,
    );
    const loaded = invoke(request(), path.dirname(directory));
    expect(loaded.status, loaded.stderr).toBe(0);
    expect(JSON.parse(loaded.stdout).assetMatches).toEqual({});
    const built = invoke(request("build"), path.dirname(directory));
    expect(built.status, built.stderr).toBe(0);
    const matches = JSON.parse(built.stdout).assetMatches;
    const expected = Object.fromEntries(
      patterns.map((pattern) => [
        pattern,
        [...new Glob(pattern, { cwd: directory })].map((match) => path.resolve(directory, match)),
      ]),
    );
    expect(matches).toEqual(expected);
    expect(Object.keys(matches)).toHaveLength(patterns.length);
    expect(matches["assets/nested"]).toEqual([path.join(directory, "assets/nested")]);
    expect(matches["generated.txt"]).toEqual([path.join(directory, "generated.txt")]);
    expect(matches["assets/*"]).not.toContain(path.join(directory, "assets/.hidden.txt"));
    expect(matches["assets/.*"]).toContain(path.join(directory, "assets/.hidden.txt"));
    expect(matches["assets/literal\\[1\\].txt"]).toEqual([
      path.join(directory, "assets/literal[1].txt"),
    ]);
  });

  it("propagates build failures without a partial JSON response", async () => {
    await writeFile(
      path.join(directory, "caido.config.ts"),
      `
      export default { ...${config}, plugins: [{ id: "ui", kind: "frontend", root: "." }] };
    `,
    );
    const result = invoke(request("build"));
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain("entry");
  });
});
