import fs from "fs/promises";
import path from "path";

import JSZip from "jszip";

/**
 * Extracts the content of a specific file from a zip archive
 * @param zipPath - Path to the zip file
 * @param filePath - The internal path of the file to extract
 * @returns Promise resolving to the file content as a string or null if not found
 */
export async function getZipFileContent(
  zipPath: string,
  filePath: string,
): Promise<string | undefined> {
  try {
    const zipBuffer = await fs.readFile(zipPath);
    const zip = new JSZip();
    const loadedZip = await zip.loadAsync(zipBuffer);
    const file = loadedZip.file(filePath);

    if (!file) {
      return undefined;
    }

    return await file.async("string");
  } catch (error) {
    console.error("Error extracting file from zip:", error);
    return undefined;
  }
}
