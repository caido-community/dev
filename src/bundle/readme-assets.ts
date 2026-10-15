import fs from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";

import encode, { init as initWebpEncoder } from "@jsquash/webp/encode.js";
import { z } from "zod";

import { logInfo } from "../utils";
import { type ReadmeEncoder, wasm } from "../wasm";

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

function normalized(file: string): string {
  return process.platform === "win32" ? file.toLowerCase() : file;
}

export async function readAsset(directory: string, pathname: string, url: string): Promise<Buffer> {
  const file = path.resolve(directory, pathname);
  const stats = await fs.lstat(file);
  const [root, realFile] = await Promise.all([fs.realpath(directory), fs.realpath(file)]);
  const relative = path.relative(normalized(root), normalized(realFile));
  if (
    stats.isSymbolicLink() ||
    !relative ||
    relative === ".." ||
    relative.startsWith(`..${path.sep}`) ||
    path.isAbsolute(relative)
  ) {
    throw new Error(`README asset path escapes project root: ${url}`);
  }
  return fs.readFile(realFile);
}

export async function compressImageToWebpDataUri(directory: string, url: string): Promise<string> {
  const input = await readAsset(directory, wasm.readme_local_path(url), url);
  const result = await wasm.convert_readme_image(input, url, encodeImage);
  logInfo(result.message);
  return result.url;
}

export async function transformReadmeImages(readmePath: string): Promise<string> {
  const content = await fs.readFile(readmePath, "utf-8");
  const result = z
    .object({
      content: z.string(),
      warnings: z.array(z.string()),
      messages: z.array(z.string()),
    })
    .parse(
      JSON.parse(
        await wasm.bundle_readme(
          content,
          (pathname, url) => readAsset(path.dirname(readmePath), pathname, url),
          encodeImage,
        ),
      ),
    );
  for (const message of [...result.messages, ...result.warnings]) logInfo(message);
  return result.content;
}
