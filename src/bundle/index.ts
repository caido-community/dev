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
  const result = wasm.package_plugin(JSON.stringify({ config, buildOutputs }), preloaded);
  for (const message of result.messages) logInfo(message);
  await host.writePackage(result);
  logSuccess("Plugin package zip file created successfully");
}
