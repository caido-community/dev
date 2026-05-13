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
const MAX_INLINED_IMAGE_BYTES = 100 * 1024;
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

  it("resizes large local images before inlining", async () => {
    await writePng(path.join(tempDir, "large.png"), 3200, 2400);

    const dataUri = await compressImageToWebpDataUri(tempDir, "large.png");
    const metadata = await sharp(decodeWebpDataUri(dataUri)).metadata();

    expect(metadata.width).toBeLessThanOrEqual(1600);
    expect(metadata.height).toBeLessThanOrEqual(1600);
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

describe("transformReadmeImages", () => {
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "readme-assets-"));
  });

  afterEach(async () => {
    await fs.rm(tempDir, { recursive: true, force: true });
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

  it("strips unsupported markdown URL schemes and protocol-relative URLs", async () => {
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        "[HTTPS](https://example.com/docs)",
        "[Protocol Relative](//example.com/docs)",
        "[JavaScript](javascript:alert(1))",
        "[Mail](mailto:test@example.com)",
        "[File](file:///tmp/test.txt)",
        "![Blob](blob:https://example.com/id)",
        "[Local Doc](assets/test.txt)",
        "[Fragment](#section)",
      ].join("\n"),
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).not.toContain("https://example.com/docs");
    expect(content).not.toContain("//example.com/docs");
    expect(content).not.toContain("javascript:alert");
    expect(content).not.toContain("mailto:test@example.com");
    expect(content).not.toContain("file:///tmp/test.txt");
    expect(content).not.toContain("blob:https://example.com/id");
    expect(content).toContain("[HTTPS]()");
    expect(content).toContain("[Protocol Relative]()");
    expect(content).toContain("[JavaScript]()");
    expect(content).toContain("[Mail]()");
    expect(content).toContain("[File]()");
    expect(content).toContain("![Blob]()");
    expect(content).toContain("[Local Doc](assets/test.txt)");
    expect(content).toContain("[Fragment](#section)");
  });

  it("preserves data image URIs only in image contexts", async () => {
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        "![Data Image](data:image/png;base64,abc123)",
        "[Data Link](data:image/png;base64,abc123)",
        "![Data SVG](data:image/svg+xml;base64,abc123)",
      ].join("\n"),
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).toContain("![Data Image](data:image/png;base64,abc123)");
    expect(content).toContain("[Data Link]()");
    expect(content).toContain("![Data SVG]()");
  });

  it("uses reference-style definition context when handling data image URIs", async () => {
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        "![Image Ref][data-image]",
        "[Link Ref][data-link]",
        "",
        "[data-image]: data:image/png;base64,abc123",
        "[data-link]: data:image/png;base64,abc123",
      ].join("\n"),
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).toMatch(/\[data-image\]:\s*data:image\/png;base64,abc123/);
    expect(content).toContain("[data-link]: <>");
  });

  it("strips oversized existing data image URIs", async () => {
    const oversizedDataUri = `data:image/png;base64,${"a".repeat(150 * 1024)}`;
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      `![Oversized](${oversizedDataUri})`,
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).toContain("![Oversized]()");
    expect(content).not.toContain(oversizedDataUri);
  });

  it("strips load-bearing raw HTML URL attributes beyond src and href", async () => {
    await writePng(path.join(tempDir, "local-image.png"), 16, 16);
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        '<video poster="https://example.com/poster.png"></video>',
        '<object data="https://example.com/embed.html"></object>',
        '<form action="https://example.com/submit"></form>',
        '<button formaction="https://example.com/submit">Send</button>',
        '<table background="https://example.com/bg.png"></table>',
        '<img srcset="https://example.com/one.png 1x, local-image.png 2x" src="local-image.png">',
        '<a style="background-image: url(https://example.com/bg.png)" href="assets/test.txt">Local</a>',
      ].join("\n"),
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).not.toContain("https://example.com");
    expect(content).toContain('poster=""');
    expect(content).toContain('data=""');
    expect(content).toContain('action=""');
    expect(content).toContain('formaction=""');
    expect(content).toContain('background=""');
    expect(content).toContain('srcset=""');
    expect(content).toContain('src="data:image/webp;base64,');
    expect(content).toContain('style=""');
    expect(content).toContain('href="assets/test.txt"');
  });

  it("preserves GitHub-flavored markdown tables and task lists", async () => {
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        "| Name | Enabled |",
        "| ---- | ------- |",
        "| Test | yes |",
        "",
        "- [x] First",
        "- [ ] Second",
      ].join("\n"),
    );

    const content = await transformReadmeImages(
      path.join(tempDir, "README.md"),
    );

    expect(content).toContain("| Name | Enabled |");
    expect(content).toMatch(/\| Test\s+\| yes\s+\|/);
    expect(content).toMatch(/[*-] \[x\] First/);
    expect(content).toMatch(/[*-] \[ \] Second/);
  });
});
