import fs from "fs/promises";
import path from "path";

import type { Definition, Html, Image, Link } from "mdast";
import remarkParse from "remark-parse";
import remarkStringify from "remark-stringify";
import sharp from "sharp";
import { unified } from "unified";
import { visit } from "unist-util-visit";

import { logInfo } from "../utils";

const MAX_INLINED_IMAGE_BYTES = 100 * 1024; // 133Kb base64 encoded
const MAX_README_BYTES = 2 * 1024 * 1024; // 2 Mb
const WEBP_MIME_TYPE = "image/webp";
const IMAGE_EXTENSIONS = new Set([
  ".avif",
  ".gif",
  ".jpeg",
  ".jpg",
  ".png",
  ".svg",
  ".tif",
  ".tiff",
  ".webp",
]);

type UrlNode = Image | Link | Definition;

/**
 * Checks if a URL is external (http or https).
 * data: URIs and fragment identifiers are not considered external.
 */
function isExternalUrl(url: string): boolean {
  if (url.startsWith("data:") || url.startsWith("#")) {
    return false;
  }
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function getLocalPathname(url: string): string {
  const markerIndex = url.search(/[?#]/);
  return markerIndex === -1 ? url : url.slice(0, markerIndex);
}

function decodeLocalPathname(url: string): string {
  try {
    return decodeURIComponent(getLocalPathname(url));
  } catch {
    return getLocalPathname(url);
  }
}

function isLocalImageUrl(url: string): boolean {
  const pathname = decodeLocalPathname(url);
  return IMAGE_EXTENSIONS.has(path.extname(pathname).toLowerCase());
}

function resolveReadmeAssetPath(readmeDir: string, url: string): string {
  const assetPath = path.resolve(readmeDir, decodeLocalPathname(url));
  const relativeAssetPath = path.relative(readmeDir, assetPath);

  if (
    relativeAssetPath === "" ||
    relativeAssetPath.startsWith("..") ||
    path.isAbsolute(relativeAssetPath)
  ) {
    throw new Error(`README asset path escapes project root: ${url}`);
  }

  return assetPath;
}

export async function compressImageToWebpDataUri(
  readmeDir: string,
  originalUrl: string,
): Promise<string> {
  const assetPath = resolveReadmeAssetPath(readmeDir, originalUrl);
  const input = await fs.readFile(assetPath);

  let bestBuffer: Buffer | undefined;
  for (const quality of [80, 70, 60, 50, 40, 30, 20]) {
    const output = await sharp(input, {
      animated: true,
      limitInputPixels: false,
    })
      .webp({ quality, effort: 6 })
      .toBuffer();

    bestBuffer = output;

    if (output.byteLength <= MAX_INLINED_IMAGE_BYTES) {
      break;
    }
  }

  if (bestBuffer === undefined) {
    throw new Error(`Unable to process README image: ${originalUrl}`);
  }

  if (bestBuffer.byteLength > MAX_INLINED_IMAGE_BYTES) {
    throw new Error(
      `README image ${originalUrl} is ${bestBuffer.byteLength} bytes after compression, which exceeds the ${MAX_INLINED_IMAGE_BYTES} byte limit`,
    );
  }

  logInfo(
    `Inlined README image as WebP: ${originalUrl} (${bestBuffer.byteLength} bytes)`,
  );

  return `data:${WEBP_MIME_TYPE};base64,${bestBuffer.toString("base64")}`;
}

/**
 * Transforms a URL on a markdown node. External URLs are removed. Local image
 * URLs are inlined as compressed WebP data URIs. Other local links are left as-is.
 */
async function transformNodeUrl(
  node: UrlNode,
  readmeDir: string,
  kind: string,
): Promise<void> {
  const originalUrl = node.url;

  if (
    !originalUrl ||
    originalUrl.startsWith("#") ||
    originalUrl.startsWith("data:")
  ) {
    return;
  }

  if (isExternalUrl(originalUrl)) {
    logInfo(`Warning: Skipping external ${kind} URL in README: ${originalUrl}`);
    node.url = "";
    return;
  }

  if (!isLocalImageUrl(originalUrl)) {
    return;
  }

  node.url = await compressImageToWebpDataUri(readmeDir, originalUrl);
}

/**
 * Transforms URLs found in raw HTML nodes (e.g., <img src="...">, <a href="...">).
 * External URLs are removed, and local image src attributes are inlined as
 * compressed WebP data URIs.
 */
async function transformHtmlNode(node: Html, readmeDir: string): Promise<void> {
  const attributeRegex = /\b(src|href)\s*=\s*(["'])([^"']*)\2/gi;
  const matches: {
    attr: string;
    quote: string;
    value: string;
    full: string;
  }[] = [];

  let match: RegExpExecArray | undefined;
  while ((match = attributeRegex.exec(node.value) ?? undefined) !== undefined) {
    matches.push({
      attr: match[1],
      quote: match[2],
      value: match[3],
      full: match[0],
    });
  }

  let updatedValue = node.value;
  for (const { attr, quote, value, full } of matches) {
    const kind = attr === "src" ? "image" : "link";

    if (!value || value.startsWith("#") || value.startsWith("data:")) {
      continue;
    }

    let replacement: string | undefined;
    if (isExternalUrl(value)) {
      logInfo(
        `Warning: Skipping external ${kind} URL in README HTML: ${value}`,
      );
      replacement = `${attr}=${quote}${quote}`;
    } else if (attr === "src" && isLocalImageUrl(value)) {
      const dataUri = await compressImageToWebpDataUri(readmeDir, value);
      replacement = `${attr}=${quote}${dataUri}${quote}`;
    }

    if (replacement !== undefined) {
      updatedValue = updatedValue.replace(full, replacement);
    }
  }

  node.value = updatedValue;
}

/**
 * Transforms local image references in README.md to compressed WebP data URIs.
 * Uses remark to parse the markdown AST and handles multiple node types:
 *   - `image`: ![alt](path)
 *   - `link`: [text](path)
 *   - `definition`: [ref]: path (reference-style images/links)
 *   - `html`: <img src="..."> and <a href="..."> in raw HTML blocks
 *
 * External URLs (http, https) are removed to prevent loading external resources.
 * data: URIs are preserved as they are self-contained.
 * Fragment-only links (#anchor) are preserved as same-document anchors.
 *
 * @param readmePath - Absolute path to the project's README.md.
 * @returns Modified README content with transformed URLs.
 */
export async function transformReadmeImages(
  readmePath: string,
): Promise<string> {
  const content = await fs.readFile(readmePath, "utf-8");
  const readmeDir = path.dirname(readmePath);

  const processor = unified().use(remarkParse).use(remarkStringify);
  const ast = processor.parse(content);

  const imageNodes: Image[] = [];
  const linkNodes: Link[] = [];
  const definitionNodes: Definition[] = [];
  const htmlNodes: Html[] = [];

  visit(ast, (node) => {
    switch (node.type) {
      case "image":
        imageNodes.push(node);
        break;
      case "link":
        linkNodes.push(node);
        break;
      case "definition":
        definitionNodes.push(node);
        break;
      case "html":
        htmlNodes.push(node);
        break;
      default:
        break;
    }
  });

  for (const node of imageNodes) {
    await transformNodeUrl(node, readmeDir, "image");
  }
  for (const node of linkNodes) {
    await transformNodeUrl(node, readmeDir, "link");
  }
  for (const node of definitionNodes) {
    await transformNodeUrl(node, readmeDir, "definition");
  }
  for (const node of htmlNodes) {
    await transformHtmlNode(node, readmeDir);
  }

  const modifiedContent = processor.stringify(ast);
  if (Buffer.byteLength(modifiedContent, "utf-8") > MAX_README_BYTES) {
    throw new Error(
      `README.md is ${Buffer.byteLength(modifiedContent, "utf-8")} bytes after inlining images, which exceeds the ${MAX_README_BYTES} byte limit`,
    );
  }

  return modifiedContent;
}
