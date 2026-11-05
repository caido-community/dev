import path from "node:path";

import { z } from "zod";

import type { BuildOutput, CaidoConfig } from "./types";

const requestSchema = z.strictObject({
  version: z.literal(1),
  operation: z.enum(["load", "build"]),
  cwd: z.string().min(1),
  config: z.string().min(1).optional(),
  expectedConfig: z.record(z.unknown()).optional(),
});

type PackagingPlugin = Pick<CaidoConfig["plugins"][number], "id" | "kind" | "root" | "name"> & {
  assets?: string[];
  backend?: { id: string } | null;
};

type AdapterResponse = {
  version: 1;
  config: Omit<CaidoConfig, "plugins"> & { plugins: PackagingPlugin[] };
  buildOutputs: BuildOutput[];
  assetMatches: Record<string, string[]>;
};

// Install before importing config/build modules: user config and build hooks may
// write directly to stdout as well as use console.log. Only this saved writer
// may emit the one-shot protocol response.
const writeProtocol = process.stdout.write.bind(process.stdout);
process.stdout.write = process.stderr.write.bind(process.stderr);

async function main(): Promise<void> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(Buffer.from(chunk));
  const request = requestSchema.parse(JSON.parse(Buffer.concat(chunks).toString("utf8")));
  const cwd = path.resolve(request.cwd);
  const { loadConfig } = await import("./config");
  const config = await loadConfig(cwd, request.config);
  const metadata: AdapterResponse["config"] = {
    id: config.id,
    name: config.name,
    description: config.description,
    version: config.version,
    author: config.author,
    plugins: config.plugins.map((plugin) => {
      const packagingPlugin: PackagingPlugin = {
        id: plugin.id,
        kind: plugin.kind,
        root: plugin.root,
      };
      if (plugin.name !== undefined) packagingPlugin.name = plugin.name;
      if ("assets" in plugin && plugin.assets !== undefined) {
        packagingPlugin.assets = plugin.assets;
      }
      if ("backend" in plugin && plugin.backend !== undefined) {
        packagingPlugin.backend = plugin.backend;
      }
      return packagingPlugin;
    }),
  };
  if (config.links !== undefined) metadata.links = config.links;
  if (config.watch !== undefined) metadata.watch = config.watch;
  const buildOutputs: BuildOutput[] = [];

  if (request.expectedConfig !== undefined) {
    const { isDeepStrictEqual } = await import("node:util");
    if (!isDeepStrictEqual(JSON.parse(JSON.stringify(metadata)), request.expectedConfig)) {
      throw new Error("Configuration metadata changed after native preflight; retry the build");
    }
  }

  if (request.operation === "build") {
    const { buildBackendPlugin, buildFrontendPlugin } = await import("./build");
    for (const plugin of config.plugins) {
      switch (plugin.kind) {
        case "frontend":
          buildOutputs.push(await buildFrontendPlugin(cwd, plugin));
          break;
        case "backend":
          buildOutputs.push(await buildBackendPlugin(cwd, plugin));
          break;
        case "workflow":
          break;
      }
    }
  }

  let assetMatches: AdapterResponse["assetMatches"] = {};
  if (request.operation === "build") {
    const { Glob } = await import("glob");
    const patterns = new Set(buildOutputs.flatMap((output) => output.assets));
    // Preserve legacy glob defaults, including directory matches. Rust validates
    // these paths and recursively collects directory contents before reading.
    assetMatches = Object.fromEntries(
      [...patterns].map((pattern) => [
        pattern,
        [...new Glob(pattern, { cwd })].map((match) => path.resolve(cwd, match)),
      ]),
    );
  }

  const response: AdapterResponse = {
    version: 1,
    config: metadata,
    buildOutputs,
    assetMatches,
  };
  writeProtocol(`${JSON.stringify(response)}\n`);
}

main().catch((error) => {
  console.error(error instanceof Error ? (error.stack ?? error.message) : String(error));
  process.exitCode = 1;
});
