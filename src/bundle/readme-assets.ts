import fs from "fs/promises";
import path from "path";

import type {
  Definition,
  Html,
  Image,
  ImageReference,
  Link,
  LinkReference,
} from "mdast";
import { type DefaultTreeAdapterMap, parseFragment, serialize } from "parse5";
import remarkGfm from "remark-gfm";
import remarkParse from "remark-parse";
import remarkStringify from "remark-stringify";
import sharp from "sharp";
import { unified } from "unified";
import { visit } from "unist-util-visit";

import { logInfo } from "../utils";

const MAX_INLINED_IMAGE_BYTES = 100 * 1024;
const MAX_INLINED_IMAGE_DATA_URI_BYTES =
  Math.ceil((MAX_INLINED_IMAGE_BYTES * 4) / 3) +
  "data:image/webp;base64,".length;
const MAX_README_BYTES = 2 * 1024 * 1024; // 2 Mb
const MAX_IMAGE_DIMENSION = 1600;
const MAX_INPUT_IMAGE_PIXELS = 40_000_000;
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
type UrlClassification =
  | "empty"
  | "fragment"
  | "data-image"
  | "local-image"
  | "local-non-image"
  | "blocked";
type UrlContext = "image" | "link" | "definition" | "resource";
type HtmlNode = DefaultTreeAdapterMap["node"];
type HtmlParentNode = DefaultTreeAdapterMap["parentNode"];
type HtmlElement = DefaultTreeAdapterMap["element"];
type HtmlTemplate = DefaultTreeAdapterMap["template"];

const DATA_IMAGE_URL_PATTERN =
  /^data:image\/(?:avif|gif|jpe?g|png|webp)(?:;[a-z0-9.+_-]+=[a-z0-9.+_-]+)*;base64,[a-z0-9+/]+={0,2}$/i;
const BLOCKED_HTML_URL_ATTRIBUTES = new Set([
  "action",
  "background",
  "data",
  "formaction",
  "href",
  "poster",
  "src",
  "xlink:href",
]);

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

function normalizeDefinitionIdentifier(identifier: string): string {
  return identifier.toUpperCase();
}

function hasUrlScheme(url: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(url);
}

function isProtocolRelativeUrl(url: string): boolean {
  return url.startsWith("//") || url.startsWith("\\\\");
}

function isAbsoluteLocalPath(url: string): boolean {
  const pathname = decodeLocalPathname(url);
  return path.isAbsolute(pathname) || path.win32.isAbsolute(pathname);
}

function isDataImageUrl(url: string): boolean {
  return (
    DATA_IMAGE_URL_PATTERN.test(url) &&
    Buffer.byteLength(url, "utf-8") <= MAX_INLINED_IMAGE_DATA_URI_BYTES
  );
}

function classifyReadmeUrl(originalUrl: string): UrlClassification {
  const url = originalUrl.trim();

  if (url === "") {
    return "empty";
  }

  if (url.startsWith("#")) {
    return "fragment";
  }

  if (url.toLowerCase().startsWith("data:")) {
    return isDataImageUrl(url) ? "data-image" : "blocked";
  }

  if (
    isProtocolRelativeUrl(url) ||
    hasUrlScheme(url) ||
    isAbsoluteLocalPath(url)
  ) {
    return "blocked";
  }

  return isLocalImageUrl(url) ? "local-image" : "local-non-image";
}

function logSkippedReadmeUrl(kind: string, url: string) {
  const displayUrl = url.length > 200 ? `${url.slice(0, 200)}...` : url;
  logInfo(`Warning: Skipping unsupported ${kind} URL in README: ${displayUrl}`);
}

function shouldPreserveClassifiedUrl(
  classification: UrlClassification,
  context: UrlContext,
): boolean {
  switch (classification) {
    case "empty":
      return true;
    case "fragment":
      return context === "link" || context === "definition";
    case "data-image":
      return context !== "link";
    case "local-non-image":
      return context === "link" || context === "definition";
    default:
      return false;
  }
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
      limitInputPixels: MAX_INPUT_IMAGE_PIXELS,
    })
      .resize({
        width: MAX_IMAGE_DIMENSION,
        height: MAX_IMAGE_DIMENSION,
        fit: "inside",
        withoutEnlargement: true,
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
 * Transforms a URL on a markdown node. Unsupported URL schemes are removed.
 * Local image URLs are inlined as compressed WebP data URIs. Other local links
 * are left as-is.
 */
async function transformNodeUrl(
  node: UrlNode,
  readmeDir: string,
  kind: string,
  context: UrlContext,
): Promise<void> {
  const originalUrl = node.url.trim();
  const classification = classifyReadmeUrl(originalUrl);

  if (shouldPreserveClassifiedUrl(classification, context)) {
    node.url = originalUrl;
    return;
  }

  if (classification === "local-image") {
    node.url = await compressImageToWebpDataUri(readmeDir, originalUrl);
    return;
  }

  logSkippedReadmeUrl(kind, originalUrl);
  node.url = "";
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
 * Unsupported URL schemes are removed, local image resource attributes are
 * inlined as compressed WebP data URIs, and broader load-bearing attributes are
 * emptied.
 */
async function transformHtmlNode(node: Html, readmeDir: string): Promise<void> {
  const fragment = parseFragment(node.value);

  async function transformElementAttributes(element: HtmlElement) {
    for (const attr of element.attrs) {
      const attrName = attr.name.toLowerCase();
      if (attrName === "style") {
        attr.value = "";
        continue;
      }

      if (attrName === "srcset") {
        attr.value = "";
        continue;
      }

      if (!BLOCKED_HTML_URL_ATTRIBUTES.has(attrName)) {
        continue;
      }

      const context: UrlContext = attrName === "href" ? "link" : "resource";
      const kind = context === "resource" ? "resource" : "link";
      const originalUrl = attr.value.trim();
      const classification = classifyReadmeUrl(originalUrl);

      if (shouldPreserveClassifiedUrl(classification, context)) {
        attr.value = originalUrl;
        continue;
      }

      if (classification === "local-image") {
        attr.value = await compressImageToWebpDataUri(readmeDir, originalUrl);
        continue;
      }

      logSkippedReadmeUrl(`HTML ${kind}`, originalUrl);
      attr.value = "";
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
 * Unsupported URL schemes are removed to prevent loading external resources.
 * Small data:image URIs are preserved in image contexts because they are
 * self-contained. Fragment-only links (#anchor) are preserved as same-document
 * anchors.
 *
 * @param readmePath - Absolute path to the project's README.md.
 * @returns Modified README content with transformed URLs.
 */
export async function transformReadmeImages(
  readmePath: string,
): Promise<string> {
  const content = await fs.readFile(readmePath, "utf-8");
  const readmeDir = path.dirname(readmePath);

  const processor = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkStringify);
  const ast = processor.parse(content);

  const imageNodes: Image[] = [];
  const linkNodes: Link[] = [];
  const definitionNodes: Definition[] = [];
  const htmlNodes: Html[] = [];
  const imageReferenceIdentifiers = new Set<string>();
  const linkReferenceIdentifiers = new Set<string>();

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
      case "imageReference":
        imageReferenceIdentifiers.add(
          normalizeDefinitionIdentifier(node.identifier),
        );
        break;
      case "linkReference":
        linkReferenceIdentifiers.add(
          normalizeDefinitionIdentifier(node.identifier),
        );
        break;
      case "html":
        htmlNodes.push(node);
        break;
      default:
        break;
    }
  });

  for (const node of imageNodes) {
    await transformNodeUrl(node, readmeDir, "image", "image");
  }
  for (const node of linkNodes) {
    await transformNodeUrl(node, readmeDir, "link", "link");
  }
  for (const node of definitionNodes) {
    const identifier = normalizeDefinitionIdentifier(node.identifier);
    const context = linkReferenceIdentifiers.has(identifier)
      ? "link"
      : imageReferenceIdentifiers.has(identifier)
        ? "image"
        : "definition";

    await transformNodeUrl(node, readmeDir, "definition", context);
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
