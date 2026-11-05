import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const executable = process.platform === "win32" ? "caido-dev.exe" : "caido-dev";

execFileSync(
  "cargo",
  ["build", "--manifest-path", "rust/Cargo.toml", "--locked", "--release", "--bin", "caido-dev"],
  { cwd: root, stdio: "inherit" },
);
const targetDirectory = process.env.CARGO_TARGET_DIR
  ? resolve(root, process.env.CARGO_TARGET_DIR)
  : join(root, "rust", "target");
const destination = join(root, "dist", "native");
mkdirSync(destination, { recursive: true });
copyFileSync(join(targetDirectory, "release", executable), join(destination, executable));
copyFileSync(join(root, "scripts", "native-launcher.mjs"), join(root, "dist", "cli.js"));
