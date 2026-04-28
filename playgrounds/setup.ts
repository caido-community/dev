import { execSync } from "child_process";
import path from "path";

import { afterAll, beforeAll, expect } from "vitest";

beforeAll(({ file }) => {
  // Get the test file path from the current test file
  const testPath = file.filepath;

  if (!testPath.split(path.sep).includes("playgrounds")) {
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
  execSync("node ../../dist/cli.js build", {
    cwd: playgroundDir,
  });
});

afterAll(() => {
  // Clean up if needed
});
