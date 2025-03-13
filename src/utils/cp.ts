import fs from "fs";
import path from "path";

import { Glob } from "glob";

/**
 * Copies a file or directory from the source to the destination.
 * Supports glob patterns.
 * @param cwd - The current working directory.
 * @param src - The source glob pattern.
 * @param dest - The destination directory.
 */
export function cp(cwd: string, src: string, dest: string) {
  const glob = new Glob(src, { cwd });

  for (const file of glob) {
    const fileSrc = path.join(cwd, file);

    // If the source is a file, we need to copy it to the destination.
    // Otherwise, we need to copy the entire directory content.
    let fileDest = dest;
    if (fs.statSync(fileSrc).isFile()) {
      const fileName = path.basename(file);
      fileDest = path.join(dest, fileName);
    }
    // eslint-disable-next-line n/no-unsupported-features/node-builtins
    fs.cpSync(fileSrc, fileDest, { recursive: true });
  }
}
