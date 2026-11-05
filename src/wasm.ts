import { createRequire } from "node:module";

import type { BuildOutput, CaidoConfig } from "./types";

const require = createRequire(import.meta.url);

// The generated module loads its adjacent WebAssembly binary synchronously.
const modulePath = import.meta.url.endsWith("/src/wasm.ts")
  ? "../dist/wasm/caido_dev.cjs"
  : "./wasm/caido_dev.cjs";
// SAFETY: modulePath selects our wasm-bindgen output built from rust/src/lib.rs;
// these signatures mirror its exported functions and callback contracts.
export const wasm = require(modulePath) as {
  parse_cli: (args: string[]) => CliResponse;
  plan_package: (input: string) => string;
  package_plugin: (input: PackageInput, preloaded: PreloadedPackageInputs) => WasmPackageResult;
  create_zip: (entries: string, contents: Uint8Array) => Uint8Array;
  readme_local_path: (url: string) => string;
  convert_readme_image: (
    input: Uint8Array,
    url: string,
    encode: ReadmeEncoder,
  ) => Promise<{ url: string; message: string }>;
  bundle_readme: (
    content: string,
    readAsset: (pathname: string, url: string) => Promise<Uint8Array>,
    encode: ReadmeEncoder,
  ) => Promise<ReadmeResult>;
  readme_assets: (content: string) => string;
  transform_readme: (content: string, replacements: string) => string;
};

type PackageInput = {
  config: Pick<CaidoConfig, "id" | "name" | "description" | "version" | "author" | "links"> & {
    plugins: { id: string; backend?: { id: string } | null }[];
  };
  buildOutputs: BuildOutput[];
};

type CliResponse =
  | { command: "build"; path?: string; config?: string }
  | { command: "watch"; path?: string; config?: string }
  | { command: "help"; help: string }
  | { command: "version" };

type ReadmeResult = {
  content: string;
  warnings: string[];
  messages: string[];
};

export type PreloadedPackageInputs = {
  files: { source: string; bytes: Uint8Array }[];
  assets: { pattern: string; files: { source: string; relative: string }[] }[];
  readme: ReadmeResult;
};

export type WasmPackageResult = {
  files: { path: string; bytes: Uint8Array }[];
  zip: Uint8Array;
  messages: string[];
};

export type ReadmeEncoder = (
  image: {
    data: Uint8ClampedArray;
    width: number;
    height: number;
    colorSpace: "srgb";
  },
  options: { quality: number; method: number; lossless: number },
) => Promise<ArrayBuffer>;
