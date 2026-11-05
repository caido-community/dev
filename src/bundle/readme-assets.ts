import fs from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";

import encode, { init as initWebpEncoder } from "@jsquash/webp/encode.js";

import { logInfo } from "../utils";
import { type ReadmeEncoder, type ReadmeResult, wasm } from "../wasm";

import { checkedProjectPath } from "./filesystem";

const require = createRequire(import.meta.url);
let encoderReady: Promise<void> | undefined;

function initializeEncoder(): Promise<void> {
  encoderReady ??= fs
    .readFile(require.resolve("@jsquash/webp/codec/enc/webp_enc.wasm"))
    .then((bytes) => WebAssembly.compile(bytes))
    .then(async (module) => {
      await initWebpEncoder(module);
    });
  return encoderReady;
}

export const encodeImage: ReadmeEncoder = async (image, options) => {
  await initializeEncoder();
  return encode(image, options);
};

export async function readAsset(directory: string, pathname: string, url: string): Promise<Buffer> {
  const root = await fs.realpath(directory);
  const file = await checkedProjectPath(
    root,
    path.resolve(directory, pathname),
    `README asset path escapes project root: ${url}`,
    false,
  );
  return fs.readFile(file);
}

export async function compressImageToWebpDataUri(directory: string, url: string): Promise<string> {
  const input = await readAsset(directory, wasm.readme_local_path(url), url);
  const result = await wasm.convert_readme_image(input, url, encodeImage);
  logInfo(result.message);
  return result.url;
}

export async function transformReadmeImages(readmePath: string): Promise<string> {
  const content = await fs.readFile(readmePath, "utf-8");
  // SAFETY: bundle_readme serializes Rust's ReadmeResult.
  const result = JSON.parse(
    await wasm.bundle_readme(
      content,
      (pathname, url) => readAsset(path.dirname(readmePath), pathname, url),
      encodeImage,
    ),
  ) as ReadmeResult;
  for (const message of [...result.messages, ...result.warnings]) logInfo(message);
  return result.content;
}
