import fs from "fs/promises";

import type { Definition, Html, Image, Link } from "mdast";
import remarkParse from "remark-parse";
import remarkStringify from "remark-stringify";
import { unified } from "unified";
import { visit } from "unist-util-visit";

import { logInfo } from "../utils";

export interface GitHubRepoInfo {
  owner: string;
  repo: string;
  commitHash: string;
}

type UrlNode = Image | Link | Definition;

/**
 * Parses a GitHub repository URL to extract owner, repo, and branch info.
 * Handles common URL formats including git+https:// and .git suffixes.
 * @param repoUrl - The repository URL from package.json or config.
 * @param branch - The branch to use for raw URLs (defaults to "main").
 * @returns Parsed GitHub repository information.
 * @throws Error if the URL is not a valid GitHub repository URL.
 */
export function parseGitHubRepoInfo(
  repoUrl: string,
  branch = "main",
): GitHubRepoInfo {
  // Clean common URL prefixes/suffixes
  const cleanedUrl = repoUrl.replace(/^git\+/, "").replace(/\.git$/, "");
  const match = cleanedUrl.match(/github\.com[:/]([^/]+)\/([^/.]+)/);

  if (!match) {
    throw new Error(`Invalid GitHub repository URL: ${repoUrl}`);
  }

  const [, owner, repo] = match;

  // Check if running in GitHub Actions
  const isGitHubActions = process.env.GITHUB_ACTIONS === "true";

  let commitHash: string;
  if (isGitHubActions) {
    // Use GITHUB_SHA environment variable (automatically set in GitHub Actions)
    const githubSha = process.env.GITHUB_SHA;
    if (!githubSha) {
      throw new Error(
        "GITHUB_SHA environment variable is not set in GitHub Actions",
      );
    }
    commitHash = githubSha;
    logInfo(`Using commit hash from GITHUB_SHA: ${commitHash}`);
  } else {
    // Not in GitHub Actions: use placeholder and log transformation
    commitHash = "LOCAL_BUILD";
    logInfo(
      `Not running in GitHub Actions, skipping commit hash fetch. README image links will use placeholder URL.`,
    );
  }

  return { owner, repo, commitHash };
}

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

/**
 * Builds a GitHub raw URL for a local asset path.
 */
function buildRawUrl(originalUrl: string, repoInfo: GitHubRepoInfo): string {
  return `https://raw.githubusercontent.com/${repoInfo.owner}/${repoInfo.repo}/${repoInfo.commitHash}/${originalUrl}`;
}

/**
 * Validates a GitHub raw URL by issuing a HEAD request.
 * Only runs in GitHub Actions to avoid local rate limits.
 * @returns true if URL is valid or validation was skipped, false if invalid.
 */
async function validateRawUrl(rawUrl: string): Promise<boolean> {
  if (process.env.GITHUB_ACTIONS !== "true") {
    return true;
  }

  try {
    const response = await fetch(rawUrl, { method: "HEAD" });
    if (!response.ok) {
      logInfo(`Warning: Asset not found at ${rawUrl}, removing reference`);
      return false;
    }
    return true;
  } catch {
    logInfo(
      `Warning: Failed to validate asset URL ${rawUrl}, removing reference`,
    );
    return false;
  }
}

/**
 * Transforms a URL on a node (image, link, or definition) to a GitHub raw URL
 * if it points to a local asset. Removes the URL if it's external or invalid.
 */
async function transformNodeUrl(
  node: UrlNode,
  repoInfo: GitHubRepoInfo,
  kind: string,
): Promise<void> {
  const originalUrl = node.url;

  // Skip empty URLs and fragment-only links (same-document anchors)
  if (!originalUrl || originalUrl.startsWith("#")) {
    return;
  }

  // data: URIs are self-contained and should be kept as-is
  if (originalUrl.startsWith("data:")) {
    return;
  }

  // External URLs are removed to prevent loading external resources
  if (isExternalUrl(originalUrl)) {
    logInfo(`Warning: Skipping external ${kind} URL in README: ${originalUrl}`);
    node.url = "";
    return;
  }

  // Construct GitHub raw URL for the local asset
  const rawUrl = buildRawUrl(originalUrl, repoInfo);

  // Validate URL reachability (only in GitHub Actions)
  const isValid = await validateRawUrl(rawUrl);
  if (!isValid) {
    node.url = "";
    return;
  }

  node.url = rawUrl;

  if (repoInfo.commitHash === "LOCAL_BUILD") {
    logInfo(
      `Would transform README ${kind} link (not in GitHub Actions): ${originalUrl} → ${rawUrl}`,
    );
  } else {
    logInfo(`Transformed README ${kind} link: ${originalUrl} → ${rawUrl}`);
  }
}

/**
 * Transforms URLs found in raw HTML nodes (e.g., <img src="...">, <a href="...">).
 * External URLs are removed (attribute value emptied), local paths are rewritten
 * to GitHub raw URLs.
 */
async function transformHtmlNode(
  node: Html,
  repoInfo: GitHubRepoInfo,
): Promise<void> {
  // Match src="..." and href="..." attributes (single or double quotes)
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

    // Skip empty, fragment-only, and data: URIs
    if (!value || value.startsWith("#") || value.startsWith("data:")) {
      continue;
    }

    let replacement: string;
    if (isExternalUrl(value)) {
      logInfo(
        `Warning: Skipping external ${kind} URL in README HTML: ${value}`,
      );
      replacement = `${attr}=${quote}${quote}`;
    } else {
      const rawUrl = buildRawUrl(value, repoInfo);
      const isValid = await validateRawUrl(rawUrl);
      if (!isValid) {
        replacement = `${attr}=${quote}${quote}`;
      } else {
        replacement = `${attr}=${quote}${rawUrl}${quote}`;
        if (repoInfo.commitHash === "LOCAL_BUILD") {
          logInfo(
            `Would transform README HTML ${kind} (not in GitHub Actions): ${value} → ${rawUrl}`,
          );
        } else {
          logInfo(`Transformed README HTML ${kind}: ${value} → ${rawUrl}`);
        }
      }
    }

    updatedValue = updatedValue.replace(full, replacement);
  }

  node.value = updatedValue;
}

/**
 * Transforms local asset references in README.md to GitHub raw URLs.
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
 * @param repoInfo - Parsed GitHub repository information.
 * @returns Modified README content with transformed URLs.
 */
export async function transformReadmeImages(
  readmePath: string,
  repoInfo: GitHubRepoInfo,
): Promise<string> {
  const content = await fs.readFile(readmePath, "utf-8");

  // Initialize unified processor with remark parse and stringify plugins
  const processor = unified().use(remarkParse).use(remarkStringify);

  // Parse markdown to AST
  const ast = processor.parse(content);

  // Collect URL-bearing nodes by type. We collect first and process afterwards
  // because unist-util-visit does not support async visitors.
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
        // Other node types do not carry URLs we need to transform.
        break;
    }
  });

  // Process each node type (async for URL validation via fetch)
  for (const node of imageNodes) {
    await transformNodeUrl(node, repoInfo, "image");
  }
  for (const node of linkNodes) {
    await transformNodeUrl(node, repoInfo, "link");
  }
  for (const node of definitionNodes) {
    await transformNodeUrl(node, repoInfo, "definition");
  }
  for (const node of htmlNodes) {
    await transformHtmlNode(node, repoInfo);
  }

  // Stringify the modified AST back to markdown
  const modifiedContent = processor.stringify(ast);
  return modifiedContent;
}
