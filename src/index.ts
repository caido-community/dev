import type { CaidoConfig } from "./types";

/** Preserve type checking without evaluating configuration or loading build tools. */
export const defineConfig = (config: CaidoConfig): CaidoConfig => config;
