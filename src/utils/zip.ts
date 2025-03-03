import { readdir, readFile, stat } from "fs/promises";
import { join } from "path";

import type JSZip from "jszip";

import { slash } from "./slash";

/**
 * Recursively adds files from a directory to a JSZip object
 * @param zip The JSZip object to add files to
 * @param dirPath The path to the directory to add
 * @param parentPath Optional parent path for nested directories
 */
export async function addDirectoryToZip(
  zip: JSZip,
  dirPath: string,
  parentPath: string = "",
) {
  const entries = await readdir(dirPath);

  for (const entry of entries) {
    const fullPath = join(dirPath, entry);
    const stats = await stat(fullPath);
    const relativePath = parentPath ? join(parentPath, entry) : entry;

    if (stats.isDirectory()) {
      await addDirectoryToZip(zip, fullPath, relativePath);
    } else {
      const content = await readFile(fullPath);
      zip.file(slash(relativePath), content);
    }
  }
}
