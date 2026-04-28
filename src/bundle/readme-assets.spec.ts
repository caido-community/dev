import fs from "fs/promises";
import os from "os";
import path from "path";

import sharp from "sharp";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { compressImageToWebpDataUri } from "./readme-assets";

const DATA_URI_PREFIX = "data:image/webp;base64,";
const MAX_INLINED_IMAGE_BYTES = 125 * 1024;

function decodeWebpDataUri(dataUri: string): Buffer {
  expect(dataUri.startsWith(DATA_URI_PREFIX)).toBe(true);
  return Buffer.from(dataUri.slice(DATA_URI_PREFIX.length), "base64");
}

async function writePng(filePath: string, width = 256, height = 256) {
  await sharp({
    create: {
      width,
      height,
      channels: 4,
      background: { r: 255, g: 0, b: 0, alpha: 1 },
    },
  })
    .png()
    .toFile(filePath);
}

describe("compressImageToWebpDataUri", () => {
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "readme-assets-"));
  });

  afterEach(async () => {
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  it("converts a local image to a WebP data URI under the per-image limit", async () => {
    await writePng(path.join(tempDir, "image.png"));

    const dataUri = await compressImageToWebpDataUri(tempDir, "image.png");
    const webp = decodeWebpDataUri(dataUri);
    const metadata = await sharp(webp).metadata();

    expect(webp.byteLength).toBeLessThanOrEqual(MAX_INLINED_IMAGE_BYTES);
    expect(metadata.format).toBe("webp");
    expect(metadata.width).toBe(256);
    expect(metadata.height).toBe(256);
  });

  it("resolves encoded local paths and ignores query and fragment markers", async () => {
    const assetDir = path.join(tempDir, "asset dir");
    await fs.mkdir(assetDir);
    await writePng(path.join(assetDir, "test image.png"), 16, 16);

    const dataUri = await compressImageToWebpDataUri(
      tempDir,
      "asset%20dir/test%20image.png?raw=true#preview",
    );
    const metadata = await sharp(decodeWebpDataUri(dataUri)).metadata();

    expect(metadata.format).toBe("webp");
    expect(metadata.width).toBe(16);
    expect(metadata.height).toBe(16);
  });

  it("rejects README image paths that escape the README directory", async () => {
    await expect(
      compressImageToWebpDataUri(tempDir, "../outside.png"),
    ).rejects.toThrow("escapes project root");
  });
});
