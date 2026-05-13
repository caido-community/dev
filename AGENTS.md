# AGENTS.md

Guidance for AI agents and contributors working in this repository.

## Project overview

This repository contains `@caido-community/dev`, a TypeScript CLI toolkit for building, watching, bundling, and packaging Caido plugins.

The published binary is `caido-dev`. The main commands are:

- `caido-dev build [path]`
- `caido-dev watch [path]`

The package is ESM-only and targets Node.js.

## Repository layout

- `src/cli.ts` — CLI entrypoint using `commander`.
- `src/commands/` — top-level command implementations.
- `src/build/` — frontend and backend plugin build logic.
  - Frontend plugins are built with Vite.
  - Backend plugins are built with tsup.
- `src/bundle/` — package assembly logic for the final plugin zip.
- `src/config.ts` — config loading via `jiti`.
- `src/types.ts` — Zod schemas and inferred TypeScript types for config and messages.
- `src/manifest.ts` — manifest creation helpers.
- `src/utils/` — filesystem, path, logging, zip, and utility helpers.
- `playgrounds/` — integration-style fixtures and tests for plugin builds.
- `.github/workflows/` — CI validation and publishing workflows.

## Development environment

Use pnpm for all package operations.

The package declares:

- Node.js `>=20`
- pnpm `>=9`

If using Mise, this repository currently pins:

- Node.js `24`
- pnpm `10.20.0`

CI validates with Node.js 20 and pnpm 9, so avoid relying on behavior that only works on newer tool versions unless the engine and CI configuration are updated together.

## Common commands

Run from the repository root:

- `pnpm install` — install dependencies.
- `pnpm build` — build the CLI/library with tsup into `dist/`.
- `pnpm typecheck` — run TypeScript with `--noEmit`.
- `pnpm test` — build first, then run Vitest playground tests.
- `pnpm lint` — run ESLint with autofix over `src` and `playgrounds`.
- `pnpm dev -- build [path]` — run the CLI directly from TypeScript.
- `pnpm dev -- watch [path]` — run the watch command directly from TypeScript.

Do not start long-running watch/dev processes unless explicitly requested. If you do run one, use a timeout or clearly tell the user it will keep running.

## Testing notes

Tests live under `playgrounds/**/*.spec.ts`.

The Vitest setup in `playgrounds/setup.ts` installs dependencies in the relevant playground and runs the built CLI against that fixture. Because of this:

- `pnpm test` can be slower than a typical unit test run.
- `pnpm test` requires a successful `pnpm build` first; the script handles this automatically.
- Tests may generate `dist/`, `node_modules/`, and other build artifacts inside playground directories.
- Do not commit generated playground artifacts unless intentionally changing fixtures.

When changing build, bundle, manifest, or config behavior, add or update playground coverage where practical.

### README and asset bundling tests

- Playground fixtures include `README.md` with image references to validate automatic README image inlining.
- Tests should verify `README.md` exists in the final zip and that referenced local README images are converted to WebP data URIs.
- Playground `README.md` files should include external URL examples (http://, https://, data:) to test warning and URL removal behavior.
- Ensure playground `README.md` files use relative local paths for test assets (e.g., `assets/test.png` for images and `assets/test.txt` for non-image local links).

## Coding standards

- Use TypeScript with strict types.
- Keep the project ESM-compatible.
- Prefer explicit types for public APIs and exported helpers.
- Keep Zod schemas in `src/types.ts` aligned with inferred TypeScript types.
- Validate external/user config with schemas rather than assuming shape.
- Prefer small, focused modules that match the existing directory structure.
- Preserve the current logging style through helpers in `src/utils/log.ts`.

## Path and platform compatibility

This package is validated on both Ubuntu and Windows in CI.

When working with paths:

- Use Node's `path` utilities for filesystem paths.
- Use the existing `slash` helper where a tool expects forward-slash paths.
- Avoid hardcoded `/` or `\` separators in logic.
- Be careful with glob patterns, Vite config paths, tsup entry paths, and zip entry names.

## Build pipeline expectations

The normal build flow is:

1. Load `caido.config.ts` with `loadConfig`.
2. Validate config with Zod.
3. Build each configured plugin:
   - `frontend` via Vite.
   - `backend` via tsup.
   - `workflow` is represented in config but currently has no build implementation.
4. Bundle build outputs and `README.md` into `dist/plugin_package/`:
   - `README.md` is always included from the project root (path is not configurable).
   - URL-bearing nodes in `README.md` are parsed using `remark` AST parsing. Handled node types:
     - `image` (`![alt](path)`)
     - `link` (`[text](path)`)
     - `definition` (`[ref]: path` for reference-style images/links)
     - `html` (raw HTML `<img src="...">` and `<a href="...">` attributes)
   - External URLs (`http://`, `https://`) trigger a warning and have their URLs removed (set to empty string) to prevent loading external resources.
   - `data:` URIs are preserved as-is (self-contained content).
   - Fragment-only links (`#anchor`) are preserved as same-document anchors.
   - Local image references are converted to compressed WebP `data:` URIs using `sharp`.
   - Local non-image links are left unchanged.
   - Inlined images must fit within the documented per-image compression limit (about 125 KiB), and the final transformed `README.md` must fit within the documented overall README size limit (about 2 MiB).
   - Existing build output bundling logic for frontend/backend plugins remains unchanged.
5. Create and validate `manifest.json` for the bundled frontend/backend outputs; README image inlining does not add manifest entries.
6. Zip the package as `dist/plugin_package.zip`.

If you change any part of this flow, update tests and documentation accordingly.

### README and asset bundling notes

- `README.md` is mandatory; the build will fail if it is missing from the project root.
- Local image references in `README.md` are converted to compressed WebP `data:` URIs during packaging.
- Transformation is applied to image URLs found in `image`, `link`, and `definition` markdown nodes, as well as `src` attributes inside raw `html` nodes.
- Local non-image links in markdown and raw HTML are left unchanged.
- External URLs (`http://`, `https://`) trigger a warning message and have their URL removed (set to empty string for markdown nodes, emptied attribute value for HTML) to prevent loading external resources in the plugin package.
- `data:` URIs are preserved as-is because they are self-contained.
- Fragment-only links (e.g., `#purpose`) are preserved as same-document anchors and are not rewritten.
- README images are compressed with `sharp`; each compressed image must fit under the documented per-image limit (about 125 KiB), and the final transformed README must fit under the documented overall size limit (about 2 MiB).
- `remark` and `unist-util-visit` are used for robust markdown parsing of `README.md`; do not replace with regex-based approaches. Raw HTML is parsed with `parse5` before `src`/`href` attributes are rewritten.
- `unist-util-visit` does not support async visitors, so nodes are collected first and then processed sequentially so image compression can `await`.

## Manifest and config changes

When changing plugin config shape or manifest output:

- Update the relevant Zod schema in `src/types.ts`.
- Update inferred/exported types as needed.
- Update manifest creation or bundling logic.
- Update playground `caido.config.ts` fixtures.
- Update expected manifest assertions in tests.
- Update `README.md` if user-facing configuration or CLI behavior changes.

## CLI behavior

The CLI should fail clearly and exit non-zero on errors.

When adding or modifying commands:

- Wire the command in `src/cli.ts`.
- Put command logic in `src/commands/`.
- Export new command modules from `src/commands/index.ts`.
- Keep user-facing option descriptions concise.
- Update `README.md` for new or changed command behavior.
- Add tests or playground coverage for meaningful behavior changes.

## Dependency guidance

- Use pnpm and keep `pnpm-lock.yaml` updated when dependencies change.
- Do not hand-edit lockfile content.
- Avoid adding dependencies for small utilities that are easy to implement locally.
- Prefer dependencies that work in ESM and support the declared Node.js version.
- `remark` and `unist-util-visit` are used for robust markdown parsing of `README.md` to extract asset references; do not replace with regex-based approaches.
- Do not hardcode secrets or tokens. The npm auth token belongs in environment configuration, not source files.

## Generated files and artifacts

Do not commit generated output unless explicitly intended.

Common generated paths include:

- `dist/`
- playground `dist/` directories
- playground `node_modules/`
- coverage output
- temporary zip/package output

## Documentation expectations

Update `README.md` when changing:

- Installation instructions.
- CLI commands or options.
- Config file behavior.
- Build/watch behavior.
- Output package structure.
- Requirements such as Node.js or pnpm versions.

Keep documentation examples compatible with pnpm and the published `caido-dev` binary.

## Pull request checklist for agents

Before finishing a non-trivial change, try to run:

1. `pnpm build`
2. `pnpm typecheck`
3. `pnpm test`
4. `pnpm lint`

If you cannot run a command, state why and describe the risk.

Also check:

- Public behavior is documented.
- Config schema changes have tests.
- Path handling remains cross-platform.
- Generated artifacts were not accidentally included.
- Lockfile changes are present only when dependencies changed.

## Security and safety

- Never commit API keys, npm tokens, credentials, or private registry tokens.
- Treat `caido.config.ts` as user-controlled input.
- Preserve validation and clear error reporting around user-provided config.
- Be cautious when adding filesystem operations; avoid deleting outside the intended project/build directories.
- Keep zip creation constrained to the expected plugin package directory.
