#!/usr/bin/env node
// build-native.mjs copies this to dist/cli.js (the package bin), next to dist/adapter.js.
import { spawn } from "node:child_process";
import { realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { constants } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** @type {Readonly<Record<string, string>>} */
export const nativeTargets = Object.freeze({
  "linux-x64": "@caido-community/dev-linux-x64",
  "linux-arm64": "@caido-community/dev-linux-arm64",
  "win32-x64": "@caido-community/dev-win32-x64",
  "darwin-arm64": "@caido-community/dev-darwin-arm64",
});

export function resolveNativeBinary({
  platform = process.platform,
  arch = process.arch,
  env = process.env,
  resolveModule = createRequire(import.meta.url).resolve,
} = {}) {
  if (env.CAIDO_DEV_BINARY) return resolve(env.CAIDO_DEV_BINARY);
  const target = `${platform}-${arch}`;
  const packageName = nativeTargets[target];
  if (!packageName) {
    throw new Error(
      `Unsupported platform ${target}. Supported targets: ${Object.keys(nativeTargets).join(", ")}. Use a supported Node.js architecture/platform, or set CAIDO_DEV_BINARY to an explicitly built native executable.`,
    );
  }
  const filename = platform === "win32" ? "caido-dev.exe" : "caido-dev";
  try {
    return resolveModule(`${packageName}/bin/${filename}`);
  } catch {
    throw new Error(
      `Cannot find the native executable from ${packageName}. Reinstall @caido-community/dev with optional dependencies enabled (run pnpm install without --no-optional, or npm install --include=optional). Do not copy node_modules across platforms. For local development, set CAIDO_DEV_BINARY to an explicitly built executable.`,
    );
  }
}

export function runLauncher(args = process.argv.slice(2)) {
  let binary;
  try {
    binary = resolveNativeBinary();
  } catch (error) {
    console.error(`caido-dev: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
    return;
  }
  const child = spawn(binary, args, {
    stdio: "inherit",
    env: {
      ...process.env,
      CAIDO_DEV_NODE: process.execPath,
      CAIDO_DEV_ADAPTER: fileURLToPath(new URL("../dist/adapter.js", import.meta.url)),
    },
    shell: false,
  });
  /** @type {NodeJS.Signals[]} */
  const signals =
    process.platform === "win32" ? ["SIGINT", "SIGTERM"] : ["SIGINT", "SIGTERM", "SIGHUP"];
  const handlers = new Map(signals.map((signal) => [signal, () => child.kill(signal)]));
  for (const [signal, handler] of handlers) process.on(signal, handler);
  let spawnFailed = false;
  child.once("error", (error) => {
    spawnFailed = true;
    console.error(
      `caido-dev: Cannot start native executable ${binary}: ${error.message}. Reinstall the platform package, or check CAIDO_DEV_BINARY and executable permissions.`,
    );
    process.exitCode = 1;
  });
  child.once("close", (code, signal) => {
    for (const [name, handler] of handlers) process.removeListener(name, handler);
    if (signal) {
      // Re-raise after removing handlers so callers observe termination by signal.
      process.exitCode = 128 + (constants.signals[signal] ?? 1);
      try {
        process.kill(process.pid, signal);
      } catch {
        // Some Windows signals cannot be re-raised; retain the conventional status.
      }
    } else if (!spawnFailed && code !== null) {
      process.exitCode = code;
    }
  });
}

if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runLauncher();
}
