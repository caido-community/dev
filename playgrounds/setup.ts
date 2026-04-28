import { execFileSync, execSync } from "child_process";
import path from "path";

import { afterAll, beforeAll, expect } from "vitest";

function hasPathSegment(filePath: string, segment: string) {
  return filePath.split(/[\\/]+/).includes(segment);
}

beforeAll(({ file }) => {
  // Get the test file path from the current test file
  const testPath = file.filepath;

  if (!hasPathSegment(testPath, "playgrounds")) {
    return;
  }

  // Find the playground directory (parent of __tests__ directory)
  const playgroundDir = path.dirname(path.dirname(testPath));

  // Installing the dependencies
  console.log(`Installing dependencies in ${playgroundDir}...`);
  execSync("pnpm install", {
    cwd: playgroundDir,
  });

  // Run pnpm build in the playground directory
  console.log(`Building playground in ${playgroundDir}...`);
  execFileSync(
    process.execPath,
    [path.join("..", "..", "dist", "cli.js"), "build"],
    {
      cwd: playgroundDir,
    },
  );
});

afterAll(() => {
  // Clean up if needed
});
