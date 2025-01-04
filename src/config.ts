import path from 'path';
import { createJiti } from 'jiti';
import { logInfo } from './utils';
import { CaidoConfig, caidoConfigSchema } from './types';

const DEFAULT_CONFIG_FILE = 'caido.config.ts';

export async function loadConfig(cwd: string, configPath?: string): Promise<CaidoConfig> {
  const configFile = configPath || path.resolve(cwd, DEFAULT_CONFIG_FILE);
  logInfo(`Loading configuration: ${configFile}`);
  
  try {
    // Use jiti for dynamic imports
    const jiti = createJiti(process.cwd(), { interopDefault: true });
    const config = await jiti.import(configFile, { default: true });
    return caidoConfigSchema.parse(config);
  } catch (error) {
    if (error instanceof Error) {
      throw new Error(`Failed to load configuration: ${error.message}`);
    }
    throw new Error('Unknown error occurred');
  }
}


export const defineConfig = (config: CaidoConfig) => {
  return config;
};
