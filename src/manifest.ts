import { validateManifest } from "@caido/plugin-manifest";

/** Validate Rust's assembled manifest against Caido's published schema. */
export function validatePackageManifest(manifest: string): boolean {
  return validateManifest(JSON.parse(manifest));
}
