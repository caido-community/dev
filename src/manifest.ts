import type { Manifest, ManifestPlugin } from '@caido/plugin-manifest';
import { CaidoConfig } from './types';

export function createManifest(options: {
    config: CaidoConfig
}): Manifest {
  const { config } = options;
  return {
    id: config.id,
    name: config.name,
    version: config.version,
    description: config.description,
    author: {
        name: config.author.name,
        email: config.author?.email,
        url: config.author?.url,
    },
    plugins: []
  };
}

export function defineFrontendPluginManifest(pluginManifest: Extract<ManifestPlugin, { kind: 'frontend' }>) {
    return pluginManifest;
}

export function defineBackendPluginManifest(pluginManifest: Extract<ManifestPlugin, { kind: "backend" }>) {
    return pluginManifest;
}