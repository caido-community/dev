import type { BuildOutput, CaidoConfig } from "../types";
import { logInfo, logSuccess } from "../utils";
import { wasm } from "../wasm";

import { createPackageHost, preloadPackageInputs } from "./package-host";

export async function bundlePackage(options: {
  cwd: string;
  buildOutputs: BuildOutput[];
  config: CaidoConfig;
}): Promise<void> {
  const { config, buildOutputs } = options;
  logInfo("Bundling plugin package");
  const host = await createPackageHost({
    cwd: options.cwd,
    pluginRoots: config.plugins.map((plugin) => plugin.root),
  });
  const preloaded = await preloadPackageInputs(host, buildOutputs);
  // Build-tool options can contain functions; only packaging fields cross into Rust.
  const packageConfig: Parameters<typeof wasm.package_plugin>[0]["config"] = {
    id: config.id,
    name: config.name,
    description: config.description,
    version: config.version,
    author: config.author,
    plugins: config.plugins.map((plugin) => {
      const packaged: Parameters<typeof wasm.package_plugin>[0]["config"]["plugins"][number] = {
        id: plugin.id,
      };
      if (plugin.kind === "frontend" && plugin.backend !== undefined) {
        packaged.backend = plugin.backend;
      }
      return packaged;
    }),
  };
  if (config.links !== undefined) packageConfig.links = config.links;
  const result = wasm.package_plugin({ config: packageConfig, buildOutputs }, preloaded);
  for (const message of result.messages) logInfo(message);
  await host.writePackage(result);
  logSuccess("Plugin package zip file created successfully");
}
