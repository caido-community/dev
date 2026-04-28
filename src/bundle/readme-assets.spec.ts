import fs from "fs/promises";
import os from "os";
import path from "path";

import sharp from "sharp";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  compressImageToWebpDataUri,
  transformReadmeImages,
} from "./readme-assets";

const DATA_URI_PREFIX = "data:image/webp;base64,";
const MAX_INLINED_IMAGE_BYTES = 125 * 1024;
const itUnix = process.platform === "win32" ? it.skip : it;
const itWindows = process.platform === "win32" ? it : it.skip;

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

  it("transforms raw HTML URLs with HTML parsing", async () => {
    await writePng(path.join(tempDir, "local-image.png"), 16, 16);
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        "<IMG SRC=local-image.png ALT='Local image'>",
        "<a HREF=https://example.com/docs>External docs</a>",
        "<img src=data:image/png;base64,abc123 alt='Data image'>",
        "<a href=#section>Section</a>",
      ].join("\n"),
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).toContain('src="data:image/webp;base64,');
    expect(content).toContain('href=""');
    expect(content).toContain('src="data:image/png;base64,abc123"');
    expect(content).toContain('href="#section"');
  });

  itWindows("resolves README image paths with Windows separators", async () => {
    const assetDir = path.join(tempDir, "asset-dir");
    await fs.mkdir(assetDir);
    await writePng(path.join(assetDir, "test-image.png"), 16, 16);

    const dataUri = await compressImageToWebpDataUri(
      tempDir,
      "asset-dir\\test-image.png",
    );
    const metadata = await sharp(decodeWebpDataUri(dataUri)).metadata();

    expect(metadata.format).toBe("webp");
    expect(metadata.width).toBe(16);
    expect(metadata.height).toBe(16);
  });

  it("rejects README image paths that resolve outside the README directory", async () => {
    const outsideDir = await fs.mkdtemp(
      path.join(os.tmpdir(), "readme-assets-outside-"),
    );

    try {
      await writePng(path.join(outsideDir, "outside.png"), 16, 16);

      await expect(
        compressImageToWebpDataUri(
          tempDir,
          path.relative(tempDir, path.join(outsideDir, "outside.png")),
        ),
      ).rejects.toThrow("escapes project root");
    } finally {
      await fs.rm(outsideDir, { recursive: true, force: true });
    }
  });

  itWindows(
    "rejects README image paths with Windows separators that resolve outside the README directory",
    async () => {
      const outsideDir = await fs.mkdtemp(
        path.join(os.tmpdir(), "readme-assets-outside-"),
      );

      try {
        await writePng(path.join(outsideDir, "outside.png"), 16, 16);

        await expect(
          compressImageToWebpDataUri(
            tempDir,
            path.win32.relative(tempDir, path.join(outsideDir, "outside.png")),
          ),
        ).rejects.toThrow("escapes project root");
      } finally {
        await fs.rm(outsideDir, { recursive: true, force: true });
      }
    },
  );

  itUnix("rejects README image paths that are symlinks", async () => {
    await writePng(path.join(tempDir, "image.png"), 16, 16);
    await fs.symlink(
      path.join(tempDir, "image.png"),
      path.join(tempDir, "image-link.png"),
    );

    await expect(
      compressImageToWebpDataUri(tempDir, "image-link.png"),
    ).rejects.toThrow("escapes project root");
  });

  itUnix(
    "rejects README image paths that escape through parent symlinks",
    async () => {
      const outsideDir = await fs.mkdtemp(
        path.join(os.tmpdir(), "readme-assets-outside-"),
      );

      try {
        await writePng(path.join(outsideDir, "outside.png"), 16, 16);
        await fs.symlink(outsideDir, path.join(tempDir, "linked-assets"));

        await expect(
          compressImageToWebpDataUri(tempDir, "linked-assets/outside.png"),
        ).rejects.toThrow("escapes project root");
      } finally {
        await fs.rm(outsideDir, { recursive: true, force: true });
      }
    },
  );
});
