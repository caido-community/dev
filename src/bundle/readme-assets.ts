import fs from "fs/promises";
import path from "path";

import type { Definition, Html, Image, Link } from "mdast";
import { type DefaultTreeAdapterMap, parseFragment, serialize } from "parse5";
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
type HtmlNode = DefaultTreeAdapterMap["node"];
type HtmlParentNode = DefaultTreeAdapterMap["parentNode"];
type HtmlElement = DefaultTreeAdapterMap["element"];
type HtmlTemplate = DefaultTreeAdapterMap["template"];

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

function normalizePathForComparison(filePath: string): string {
  const normalizedPath = path.resolve(filePath);
  return process.platform === "win32"
    ? normalizedPath.toLowerCase()
    : normalizedPath;
}

function isPathInDirectory(directory: string, filePath: string): boolean {
  const normalizedDirectory = normalizePathForComparison(directory);
  const normalizedFilePath = normalizePathForComparison(filePath);
  const directoryWithSeparator = normalizedDirectory.endsWith(path.sep)
    ? normalizedDirectory
    : `${normalizedDirectory}${path.sep}`;

  return normalizedFilePath.startsWith(directoryWithSeparator);
}

async function resolveReadmeAssetPath(
  readmeDir: string,
  url: string,
): Promise<string> {
  const assetPath = path.resolve(readmeDir, decodeLocalPathname(url));
  const assetStats = await fs.lstat(assetPath);

  if (assetStats.isSymbolicLink()) {
    throw new Error(`README asset path escapes project root: ${url}`);
  }

  const [realReadmeDir, realAssetPath] = await Promise.all([
    fs.realpath(readmeDir),
    fs.realpath(assetPath),
  ]);

  if (!isPathInDirectory(realReadmeDir, realAssetPath)) {
    throw new Error(`README asset path escapes project root: ${url}`);
  }

  return realAssetPath;
}

export async function compressImageToWebpDataUri(
  readmeDir: string,
  originalUrl: string,
): Promise<string> {
  const assetPath = await resolveReadmeAssetPath(readmeDir, originalUrl);
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

function isHtmlElement(node: HtmlNode): node is HtmlElement {
  return "attrs" in node;
}

function isHtmlParentNode(node: HtmlNode): node is HtmlParentNode {
  return "childNodes" in node;
}

function isHtmlTemplate(node: HtmlNode): node is HtmlTemplate {
  return "content" in node;
}

/**
 * Transforms URLs found in raw HTML nodes (e.g., <img src="...">, <a href="...">).
 * External URLs are removed, and local image src attributes are inlined as
 * compressed WebP data URIs.
 */
async function transformHtmlNode(node: Html, readmeDir: string): Promise<void> {
  const fragment = parseFragment(node.value);

  async function transformElementAttributes(element: HtmlElement) {
    for (const attr of element.attrs) {
      const attrName = attr.name.toLowerCase();
      if (attrName !== "src" && attrName !== "href") {
        continue;
      }

      const kind = attrName === "src" ? "image" : "link";
      if (
        !attr.value ||
        attr.value.startsWith("#") ||
        attr.value.startsWith("data:")
      ) {
        continue;
      }

      if (isExternalUrl(attr.value)) {
        logInfo(
          `Warning: Skipping external ${kind} URL in README HTML: ${attr.value}`,
        );
        attr.value = "";
      } else if (attrName === "src" && isLocalImageUrl(attr.value)) {
        attr.value = await compressImageToWebpDataUri(readmeDir, attr.value);
      }
    }
  }

  async function walkHtmlNodes(parentNode: HtmlParentNode) {
    for (const childNode of parentNode.childNodes) {
      if (isHtmlElement(childNode)) {
        await transformElementAttributes(childNode);
      }

      if (isHtmlParentNode(childNode)) {
        await walkHtmlNodes(childNode);
      }

      if (isHtmlTemplate(childNode)) {
        await walkHtmlNodes(childNode.content);
      }
    }
  }

  await walkHtmlNodes(fragment);

  node.value = serialize(fragment);
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
