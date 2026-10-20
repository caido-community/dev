import fs from "node:fs/promises";
import path from "node:path";

import { Glob } from "glob";

/** Enumerate copy inputs without choosing package destinations or writing files. */
export async function expandPackageAssets(
  cwd: string,
  pattern: string,
  checkedSource: (source: string) => Promise<string>,
): Promise<{ source: string; relative: string }[]> {
  const files: { source: string; relative: string }[] = [];
  const collected = new Map<string, Set<string>>();
  async function collect(source: string, relative: string): Promise<void> {
    const file = await checkedSource(source);
    const stats = await fs.stat(file);
    if (stats.isDirectory()) {
      for (const child of await fs.readdir(file)) {
        await collect(path.join(source, child), relative ? `${relative}/${child}` : child);
      }
    } else if (stats.isFile()) {
      // Directory traversal and a direct glob match may identify the same copy.
      const destinations = collected.get(file) ?? new Set<string>();
      if (destinations.has(relative)) return;
      destinations.add(relative);
      collected.set(file, destinations);
      files.push({ source: file, relative });
    } else {
      throw new Error(`Unsupported package source: ${source}`);
    }
  }
  // Match recursive copy semantics: files use their basename; directories copy contents.
  for (const match of new Glob(pattern, { cwd })) {
    const source = path.resolve(cwd, match);
    const file = await checkedSource(source);
    await collect(source, (await fs.stat(file)).isFile() ? path.basename(match) : "");
  }
  return files;
}
