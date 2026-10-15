import fs from "fs/promises";
import os from "os";
import path from "path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { compressImageToWebpDataUri, transformReadmeImages } from "./readme-assets";
const imageFixture = (name: string) => new URL(`./__fixtures__/images/${name}`, import.meta.url);

const DATA_URI_PREFIX = "data:image/webp;base64,";
const MAX_INLINED_IMAGE_BYTES = 100 * 1024;
const itUnix = process.platform === "win32" ? it.skip : it;
const itWindows = process.platform === "win32" ? it : it.skip;

function decodeWebpDataUri(dataUri: string): Buffer {
  expect(dataUri.startsWith(DATA_URI_PREFIX)).toBe(true);
  return Buffer.from(dataUri.slice(DATA_URI_PREFIX.length), "base64");
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
    await fs.copyFile(imageFixture("red256x256.png"), path.join(tempDir, "image.png"));
    const source = await fs.readFile(path.join(tempDir, "image.png"));

    const dataUri = await compressImageToWebpDataUri(tempDir, "image.png");
    const webp = decodeWebpDataUri(dataUri);

    expect(webp.byteLength).toBeLessThanOrEqual(MAX_INLINED_IMAGE_BYTES);
    expect(webp.byteLength).toBeLessThan(source.byteLength);
  });

  it("converts a large image to a small output", async () => {
    await fs.copyFile(imageFixture("red3200x2400.png"), path.join(tempDir, "large.png"));
    const source = await fs.readFile(path.join(tempDir, "large.png"));

    const dataUri = await compressImageToWebpDataUri(tempDir, "large.png");
    const webp = decodeWebpDataUri(dataUri);

    expect(webp.byteLength).toBeLessThanOrEqual(MAX_INLINED_IMAGE_BYTES);
    expect(webp.byteLength).toBeLessThan(source.byteLength);
  });

  it("resolves encoded local paths and ignores query and fragment markers", async () => {
    const assetDir = path.join(tempDir, "asset dir");
    await fs.mkdir(assetDir);
    await fs.copyFile(imageFixture("red16x16.png"), path.join(assetDir, "test image.png"));

    const dataUri = await compressImageToWebpDataUri(
      tempDir,
      "asset%20dir/test%20image.png?raw=true#preview",
    );

    expect(decodeWebpDataUri(dataUri).byteLength).toBeGreaterThan(0);
  });

  itWindows("resolves README image paths with Windows separators", async () => {
    const assetDir = path.join(tempDir, "asset-dir");
    await fs.mkdir(assetDir);
    await fs.copyFile(imageFixture("red16x16.png"), path.join(assetDir, "test-image.png"));

    const dataUri = await compressImageToWebpDataUri(tempDir, "asset-dir\\test-image.png");

    expect(decodeWebpDataUri(dataUri).byteLength).toBeGreaterThan(0);
  });

  it("rejects README image paths that resolve outside the README directory", async () => {
    const outsideDir = await fs.mkdtemp(path.join(os.tmpdir(), "readme-assets-outside-"));

    try {
      await fs.copyFile(imageFixture("red16x16.png"), path.join(outsideDir, "outside.png"));

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
      const outsideDir = await fs.mkdtemp(path.join(os.tmpdir(), "readme-assets-outside-"));

      try {
        await fs.copyFile(imageFixture("red16x16.png"), path.join(outsideDir, "outside.png"));

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
    await fs.copyFile(imageFixture("red16x16.png"), path.join(tempDir, "image.png"));
    await fs.symlink(path.join(tempDir, "image.png"), path.join(tempDir, "image-link.png"));

    await expect(compressImageToWebpDataUri(tempDir, "image-link.png")).rejects.toThrow(
      "escapes project root",
    );
  });

  itUnix("rejects README image paths that escape through parent symlinks", async () => {
    const outsideDir = await fs.mkdtemp(path.join(os.tmpdir(), "readme-assets-outside-"));

    try {
      await fs.copyFile(imageFixture("red16x16.png"), path.join(outsideDir, "outside.png"));
      await fs.symlink(outsideDir, path.join(tempDir, "linked-assets"));

      await expect(
        compressImageToWebpDataUri(tempDir, "linked-assets/outside.png"),
      ).rejects.toThrow("escapes project root");
    } finally {
      await fs.rm(outsideDir, { recursive: true, force: true });
    }
  });
});

describe("README GIF packaging", () => {
  it.each([
    { fixture: "animated.gif", preserved: true },
    { fixture: "large.gif", preserved: false },
    { fixture: "animated.gif", preserved: false, oversized: true },
  ])("inlines $fixture with preservation $preserved", async ({ fixture, preserved, oversized }) => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "readme-gif-"));
    try {
      let input = await fs.readFile(new URL(`./__fixtures__/images/${fixture}`, import.meta.url));
      if (oversized) input = Buffer.concat([input, Buffer.alloc(MAX_INLINED_IMAGE_BYTES)]);
      await fs.writeFile(path.join(directory, "image.gif"), input);
      const readme = path.join(directory, "README.md");
      await fs.writeFile(readme, "![Animation](image.gif)");
      const output = await transformReadmeImages(readme);
      if (preserved) {
        expect(output).toContain(`data:image/gif;base64,${input.toString("base64")}`);
      } else {
        const encoded = output.match(/data:image\/webp;base64,([A-Za-z0-9+/=]+)/)?.[1];
        expect(encoded).toBeDefined();
        expect(Buffer.from(encoded!, "base64").byteLength).toBeLessThanOrEqual(
          MAX_INLINED_IMAGE_BYTES,
        );
      }
    } finally {
      await fs.rm(directory, { recursive: true, force: true });
    }
  });
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
    await fs.copyFile(imageFixture("red16x16.png"), path.join(tempDir, "local-image.png"));
    await fs.writeFile(
      path.join(tempDir, "README.md"),
      [
        "<IMG SRC=local-image.png ALT='Local image'>",
        "<a HREF=https://example.com/docs>External docs</a>",
        "<img src=data:image/png;base64,abc123 alt='Data image'>",
        "<a href=#section>Section</a>",
        'before <a href="#foo">click</a> after',
        'before <a href="java&#x09;script:alert(1)">unsafe</a> after',
      ].join("\n"),
    );

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

    expect(content).toContain('src="data:image/webp;base64,');
    expect(content).toContain('href=""');
    expect(content).toContain('src="data:image/png;base64,abc123"');
    expect(content).toContain('href="#section"');
    expect(content).toContain('before <a href="#foo">click</a> after');
    expect(content).toContain('before <a href="">unsafe</a> after');
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

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

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

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

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
        "",
        "![Shared Image][shared] and [Shared Link][shared]",
        "",
        "[shared]: data:image/png;base64,abc123",
      ].join("\n"),
    );

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

    expect(content).toContain("![Image Ref](data:image/png;base64,abc123)");
    expect(content).toContain("[Link Ref]()");
    expect(content).toContain("![Shared Image](data:image/png;base64,abc123)");
    expect(content).toContain("[Shared Link]()");
    expect(content).not.toContain("[shared]:");
  });

  it("rejects a README larger than 2 MiB", async () => {
    await fs.writeFile(path.join(tempDir, "README.md"), "x".repeat(2 * 1024 * 1024 + 1));

    await expect(transformReadmeImages(path.join(tempDir, "README.md"))).rejects.toThrow();
  });

  it("rejects SVG files referenced by a Markdown image", async () => {
    await fs.writeFile(
      path.join(tempDir, "image.svg"),
      '<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"></svg>',
    );
    await fs.writeFile(path.join(tempDir, "README.md"), "![SVG](image.svg)");

    await expect(transformReadmeImages(path.join(tempDir, "README.md"))).rejects.toThrow(
      /SVG|Unsupported/i,
    );
  });

  it("blocks external URLs continued inside a blockquote", async () => {
    await fs.writeFile(path.join(tempDir, "README.md"), "> [x](\n> https://example.com\n> )");

    expect(await transformReadmeImages(path.join(tempDir, "README.md"))).toBe("> [x]()\n");
  });

  it("inlines local images without changing list prefixes", async () => {
    await fs.copyFile(imageFixture("red16x16.png"), path.join(tempDir, "image.png"));
    const source = "- ![x](\n  image.png\n  )";
    await fs.writeFile(path.join(tempDir, "README.md"), source);

    const output = await transformReadmeImages(path.join(tempDir, "README.md"));
    const uri = output.match(/data:image\/webp;base64,[A-Za-z0-9+/=]+/)?.[0];
    expect(uri).toBeDefined();
    expect(output.replace(uri!, "image.png")).toMatch(/^[*-] !\[x\]\(image\.png\)\n$/);
  });

  it("blocks HTML image URLs without changing blockquote rendering", async () => {
    const source = '> <div>\n> hello\n> <img src="https://example.com/a.png">\n> </div>';
    await fs.writeFile(path.join(tempDir, "README.md"), source);

    expect(await transformReadmeImages(path.join(tempDir, "README.md"))).toBe(
      `> \n> \n${source.replace("https://example.com/a.png", "")}\n`,
    );
  });

  it("strips oversized existing data image URIs", async () => {
    const oversizedDataUri = `data:image/png;base64,${"a".repeat(150 * 1024)}`;
    await fs.writeFile(path.join(tempDir, "README.md"), `![Oversized](${oversizedDataUri})`);

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

    expect(content).toContain("![Oversized]()");
    expect(content).not.toContain(oversizedDataUri);
  });

  it("strips load-bearing raw HTML URL attributes beyond src and href", async () => {
    await fs.copyFile(imageFixture("red16x16.png"), path.join(tempDir, "local-image.png"));
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

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

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

    const content = await transformReadmeImages(path.join(tempDir, "README.md"));

    expect(content).toContain("| Name | Enabled |");
    expect(content).toMatch(/\| Test\s+\| yes\s+\|/);
    expect(content).toMatch(/[*-] \[x\] First/);
    expect(content).toMatch(/[*-] \[ \] Second/);
  });
});
