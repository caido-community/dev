import { execFile } from "node:child_process";
import { readFile, readdir } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execute = promisify(execFile);
const manifest = fileURLToPath(new URL("../rust/Cargo.toml", import.meta.url));
const noticeName = /^(?:licen[sc]e|copying|notice|authors|patents|unlicense)(?:[._-]|$)/i;
const licenseName = /^(?:licen[sc]e|copying|unlicense)(?:[._-]|$)/i;
const requiredFiles = {
  "webp@0.3.1": ["LICENSE-MIT", "LICENSE-APACHE"],
  "libwebp-sys@0.9.6": ["vendor/COPYING", "vendor/PATENTS", "vendor/AUTHORS"],
  "notify@8.2.0": ["LICENSE-CC0"],
  "inotify@0.11.5": ["LICENSE"],
  "inotify-sys@0.1.8": ["LICENSE"],
};
const headerOnly = new Set(["zune-core@0.4.12", "zune-jpeg@0.4.21"]);
const authorsLicense = new Set(["r-efi@5.3.0", "r-efi@6.0.0"]);

/** Locate all resolved source roots without assuming a Cargo home or host target. */
export async function nativeCargoMetadata() {
  const { stdout } = await execute(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--offline", "--manifest-path", manifest],
    { maxBuffer: 64 * 1024 * 1024 },
  );
  return JSON.parse(stdout);
}

async function sourceFiles(directory) {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) result.push(...(await sourceFiles(path)));
    else if (entry.isFile()) result.push(path);
    else throw new Error(`Unsupported source entry while collecting notices: ${path}`);
  }
  return result.sort((a, b) => a.localeCompare(b, "en"));
}

async function material(path) {
  const text = await readFile(path, "utf8");
  if (!text.trim()) throw new Error(`Empty third-party notice: ${path}`);
  return text;
}

/**
 * Include the entire resolved metadata dependency superset (all targets, including
 * build/dev dependencies), rather than risk omitting a non-host binary's notices.
 * Text is copied from Cargo's locked, offline source archives, never synthesized
 * from SPDX identifiers. This is notice assembly, not a replacement for cargo-deny.
 */
export async function collectNativeNotices(metadata) {
  const graph = metadata ?? (await nativeCargoMetadata());
  if (!graph.resolve || !graph.packages?.length || !graph.workspace_members?.length) {
    throw new Error("Cargo metadata must include the resolved dependency graph.");
  }
  const workspace = new Set(graph.workspace_members);
  const packages = graph.packages
    .filter((pkg) => !workspace.has(pkg.id))
    .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`, "en"));
  if (!packages.length) throw new Error("Cargo metadata contains no third-party dependencies.");
  const apache = packages.find((pkg) => pkg.name === "webp" && pkg.version === "0.3.1");
  const sections = [
    "THIRD-PARTY NOTICES\n\nThis executable includes third-party software. The following materials are copied\nfrom the locked Cargo dependency sources. This conservative superset includes\ndependencies for other targets and build/development tools. The project's MIT\nlicense does not replace these third-party terms.\n",
  ];
  for (const pkg of packages) {
    const key = `${pkg.name}@${pkg.version}`;
    const directory = dirname(pkg.manifest_path);
    const files = await sourceFiles(directory);
    const named = new Map(
      files.map((path) => [relative(directory, path).split(sep).join("/"), path]),
    );
    for (const name of requiredFiles[key] ?? []) {
      if (!named.has(name)) throw new Error(`Missing required notice for ${key}: ${name}`);
    }
    const notices = files.filter((path) => noticeName.test(path.split(sep).at(-1)));
    if (pkg.license_file) {
      const declared = resolve(directory, pkg.license_file);
      if (!files.includes(declared)) {
        throw new Error(`Missing declared license file for ${key}: ${pkg.license_file}`);
      }
      if (!notices.includes(declared)) notices.push(declared);
    }
    let hasLicense =
      Boolean(pkg.license_file) || notices.some((path) => licenseName.test(path.split(sep).at(-1)));
    const parts = [];
    for (const path of notices) {
      const text = await material(path);
      if (authorsLicense.has(key) && path === join(directory, "AUTHORS")) {
        if (!text.includes("Permission is hereby granted")) {
          throw new Error(`Missing MIT permission text in ${key} AUTHORS`);
        }
        hasLicense = true;
      }
      parts.push(`--- ${relative(directory, path).split(sep).join("/")} ---\n${text}`);
    }
    // These published archives omit standalone license files. Preserve every
    // copyright-bearing source header and use the offered Apache-2.0 option.
    // The complete standard license text is copied from the resolved webp crate.
    if (headerOnly.has(key)) {
      if (!apache) throw new Error(`Missing resolved Apache-2.0 text provider for ${key}`);
      let headers = 0;
      for (const path of files.filter((file) => file.endsWith(".rs"))) {
        const text = await readFile(path, "utf8");
        const header = /^\s*(\/\*[\s\S]*?\*\/)/.exec(text)?.[1];
        if (header?.includes("Copyright")) {
          parts.push(
            `--- ${relative(directory, path).split(sep).join("/")} (source header) ---\n${header}`,
          );
          headers++;
        }
      }
      if (!headers) throw new Error(`Missing copyright source headers for ${key}`);
      parts.push(
        `--- Apache-2.0 license text (from webp@0.3.1/LICENSE-APACHE) ---\n${await material(join(dirname(apache.manifest_path), "LICENSE-APACHE"))}`,
      );
      hasLicense = true;
    }
    if (!hasLicense) throw new Error(`No license text found for ${key} in ${directory}`);
    sections.push(
      `\n${"=".repeat(78)}\n${key}\nDeclared license: ${pkg.license ?? "see license file"}\nSource: ${pkg.source ?? "path dependency"}\n${parts.join("\n\n")}\n`,
    );
  }
  return sections.join("\n");
}
