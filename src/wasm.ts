import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

// The generated module loads its adjacent WebAssembly binary synchronously.
const modulePath = import.meta.url.endsWith("/src/wasm.ts")
  ? "../dist/wasm/caido_dev.cjs"
  : "./wasm/caido_dev.cjs";
export const wasm = require(modulePath) as {
  parse_cli: (args: string) => string;
  plan_package: (input: string) => string;
  package_plugin: (
    input: string,
    readFile: (source: string) => Promise<Uint8Array>,
    expandAssets: (
      pattern: string,
    ) => Promise<{ source: string; relative: string }[]>,
    readAsset: (pathname: string, url: string) => Promise<Uint8Array>,
    encode: ReadmeEncoder,
    validateManifest: (manifest: string) => boolean,
  ) => Promise<{
    files: { path: string; bytes: Uint8Array }[];
    zip: Uint8Array;
    messages: string[];
  }>;
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
  ) => Promise<string>;
  readme_assets: (content: string) => string;
  transform_readme: (content: string, replacements: string) => string;
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
