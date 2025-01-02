import path from 'path';
import { defineConfig, build } from 'vite';
import { existsSync } from 'fs';
import type { FrontendBuildOutput, FrontendPluginConfig } from '../types';
import { getPluginPackageJson } from '../package';
import { logInfo } from '../utils';

/**
 * Creates a Vite config for the frontend plugin.
 * @param cwd - The current working directory.
 * @param plugin - The frontend plugin configuration.
 * @returns The Vite config.
 */
function createViteConfig(cwd: string, plugin: FrontendPluginConfig) {
  // Set the entry point
  const root = path.resolve(cwd, plugin.root);
  return defineConfig({
    root,
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      lib: {
        entry: 'src/index.ts',
        formats: ['es'],
        fileName: () => 'index.js',
        cssFileName: 'index'
      },
      rollupOptions: {
        external: ['@caido/frontend-sdk']
      }
    }
  })
} 

/**
 * Builds the frontend plugin.
 * @param cwd - The current working directory.
 * @param pluginConfig - The frontend plugin configuration.
 * @returns The build output.
 */
export async function buildFrontendPlugin(cwd: string, pluginConfig: FrontendPluginConfig): Promise<FrontendBuildOutput> {
    const pluginRoot = path.resolve(cwd, pluginConfig.root);

    logInfo(`Building frontend plugin: ${pluginRoot}`);
    const viteConfig = createViteConfig(cwd, pluginConfig);
    await build(viteConfig);

    const hasCss = existsSync(`${pluginRoot}/dist/index.css`);
    const packageJson = getPluginPackageJson(pluginRoot);

    logInfo(`Frontend plugin built successfully`);

    return {
      kind: 'frontend',
      id: packageJson.name,
      name: pluginConfig.name ?? packageJson.name,
      fileName: path.join(pluginRoot, 'dist', 'index.js'),
      cssFileName: hasCss ? path.join(pluginRoot, 'dist', 'index.css') : undefined,
      backendId: pluginConfig.backend?.id
    }
}