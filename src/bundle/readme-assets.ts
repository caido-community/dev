import fs from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";

import encode, { init as initWebpEncoder } from "@jsquash/webp/encode.js";

import { logInfo } from "../utils";
import { type ReadmeEncoder, wasm } from "../wasm";

import { checkedProjectPath } from "./filesystem";

const MAX_LOCAL_IMAGE_BYTES = 64 * 1024 * 1024;
const READ_CHUNK_BYTES = 64 * 1024;

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
  const handle = await fs.open(file, "r");
  try {
    const oversized = () =>
      new Error(
        `README image ${url} exceeds the 64 MiB (${MAX_LOCAL_IMAGE_BYTES} byte) source limit`,
      );
    if ((await handle.stat()).size > MAX_LOCAL_IMAGE_BYTES) throw oversized();

    const chunks: Buffer[] = [];
    let total = 0;
    // Read one extra byte to detect growth after stat without an unbounded read.
    while (total <= MAX_LOCAL_IMAGE_BYTES) {
      const chunk = Buffer.allocUnsafe(
        Math.min(READ_CHUNK_BYTES, MAX_LOCAL_IMAGE_BYTES + 1 - total),
      );
      const { bytesRead } = await handle.read({
        buffer: chunk,
        offset: 0,
        length: chunk.length,
        position: null,
      });
      if (bytesRead === 0) break;
      total += bytesRead;
      if (total > MAX_LOCAL_IMAGE_BYTES) throw oversized();
      chunks.push(chunk.subarray(0, bytesRead));
    }
    return Buffer.concat(chunks, total);
  } finally {
    await handle.close();
  }
}

export async function compressImageToWebpDataUri(directory: string, url: string): Promise<string> {
  const input = await readAsset(directory, wasm.readme_local_path(url), url);
  const result = await wasm.convert_readme_image(input, url, encodeImage);
  logInfo(result.message);
  return result.url;
}

export async function transformReadmeImages(readmePath: string): Promise<string> {
  const content = await fs.readFile(readmePath, "utf-8");
  const result = await wasm.bundle_readme(
    content,
    (pathname, url) => readAsset(path.dirname(readmePath), pathname, url),
    encodeImage,
  );
  for (const message of [...result.messages, ...result.warnings]) logInfo(message);
  return result.content;
}
