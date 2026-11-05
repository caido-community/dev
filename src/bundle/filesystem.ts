import fs from "node:fs/promises";
import path from "node:path";

export function normalized(file: string): string {
  return process.platform === "win32" ? file.toLowerCase() : file;
}

export function contains(root: string, file: string): boolean {
  const relative = path.relative(normalized(root), normalized(file));
  return relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative);
}

/** Check the leaf itself as well as its destination through any parent symlinks. */
export async function checkedProjectPath(
  root: string,
  source: string,
  error: string,
  allowRoot = true,
): Promise<string> {
  const file = path.resolve(root, source);
  const stats = await fs.lstat(file);
  const realFile = await fs.realpath(file);
  if (
    stats.isSymbolicLink() ||
    !contains(root, realFile) ||
    (!allowRoot && normalized(root) === normalized(realFile))
  ) {
    throw new Error(error);
  }
  return realFile;
}
