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

Use Mise for repository tasks. `.mise/config.toml` and `.mise/mise.lock` pin the shared tools used for both development and community plugin testing: Node.js `24`, pnpm `11.18.0`, Rust `1.99.0` with the `wasm32-unknown-unknown` target, and wasm-bindgen CLI `0.2.104`. `.mise/config.dev.toml` and `.mise/mise.dev.lock` add cargo-deny `0.19.1` and zizmor `1.30.1`; `.mise/config.community.toml` and `.mise/mise.community.lock` separately add jq and ouch. Environments are opt-in: use `mise --env dev` for the full development toolset, or set `MISE_ENV=dev`. A task suffix such as `:dev` does not activate that environment. Shared build/test tasks still work without an environment. Task definitions are executable scripts under `.mise/tasks/` (for example, `.mise/tasks/format/dev` defines `format:dev`).

Validation CI uses `MISE_ENV=dev` on Ubuntu and Windows, and publishing uses it for the Rust audit. The published package still supports Node.js 20 and pnpm 9, so avoid introducing runtime behavior requiring newer versions without updating the package engines.

## Common commands

Run from the repository root:

- `mise --env dev install --locked` — install shared tools, the WASM target, and dev audit tools from their lockfiles.
- `mise exec -- pnpm install` — install dependencies.
- `mise run build:dev` — build JavaScript and Rust/WASM into `dist/`.
- `mise run typecheck:dev` — run TypeScript with `--noEmit`.
- `mise run test` — build first, then run Vitest once (never watch).
- `mise --env community run test-community` — Linux-only, opt-in Bash integration test: package every unique `caido-community` repository in the live store with this checkout's CLI, curl/jq/Git/pnpm, and ouch archive checks. Install extra tools with `mise --env community install --locked`; `.mise/config.community.toml` and `.mise/mise.community.lock` isolate jq/ouch from normal CI. Runs third-party install/build code; use a disposable environment. Use `-- --repository caido-community/name` for one listed repository; failures retain temporary clones and logs.
- `mise run format:dev` — format supported repository files with Oxfmt and format Rust.
- `mise run format:prod` — check Oxfmt and Rust formatting without modifying files.
- `mise run lint:dev` — apply safe Oxlint fixes, including Deslop rules.
- `mise run lint:prod` — check Oxlint and Deslop rules without modifying files.
- `mise --env dev run audit:dev` — run cargo-deny checks (both audit variants require the dev environment).
- `mise --env dev run zizmor` — audit GitHub Actions workflows (single check-only task; set `GH_TOKEN` for online audits).
- `mise run knip:dev` — check unused code and dependencies. `knip.json` lists Oxfmt as a tooling dependency because it is invoked only by the Mise shell tasks.
- `mise run cli:dev -- build [path]` — run the CLI directly from TypeScript.
- `mise run cli:dev -- watch [path]` — run the watch command directly from TypeScript.
- `mise run cli:prod -- build [path]` — build and run the packaged CLI.

CI uses `:prod` variants; build, typecheck, audit, and knip perform the same work in both variants. Linting and formatting are separate checks. Oxlint enables the full Deslop recommended configuration, including type-aware rules through `oxlint-tsgolint`; optional Deslop rules are not enabled. Keep Oxlint aligned with Deslop's exact `@oxlint/plugins` version. Development requires Node.js 20.19+ or 22.12+ for the Oxc tools (Mise supplies Node.js 24); the published CLI still supports Node.js 20. `test` has no variants and depends on `build:prod`.

Do not start long-running watch/dev processes unless explicitly requested. If you do run one, use a timeout or clearly tell the user it will keep running.

## Testing notes

Tests live under `playgrounds/**/*.spec.ts`.

The Vitest setup in `playgrounds/setup.ts` installs dependencies in the relevant playground and runs the built CLI against that fixture. Because of this:

- `mise run test` can be slower than a typical unit test run.
- `mise run test` requires a successful `build:prod` first; the task dependency handles this automatically.
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
   - Comrak parses `README.md` in Rust/WASM. Image and link URLs are transformed in its syntax tree; reference-style uses are resolved into ordinary image/link nodes. Raw HTML URL attributes are transformed using html5ever tokenization and serialization.
   - Comrak serializes the transformed tree as Markdown. Formatting is normalized, reference-style uses may become inline links/images, and unused definitions disappear.
   - External URLs (`http://`, `https://`) trigger a warning and have their URLs removed (set to empty string) to prevent loading external resources.
   - `data:` URIs are preserved as-is (self-contained content).
   - Fragment-only links (`#anchor`) are preserved as same-document anchors.
   - Local PNG, JPEG, GIF, and WebP image references are decoded and resized in Rust/WASM, then converted to lossy WebP `data:` URIs with the prebuilt `@jsquash/webp` WASM encoder. GIFs within 1600×1600 pixels and 100 KiB are inlined unchanged to preserve animation; larger GIFs and animated WebP use first-frame WebP conversion. SVG, AVIF, and TIFF are unsupported.
   - Local non-image links are left unchanged.
   - Inlined images must fit within the documented per-image compression limit (100 KiB before base64 encoding), and the final transformed `README.md` must fit within the documented overall README size limit (about 2 MiB).
   - Existing build output bundling logic for frontend/backend plugins remains unchanged.
5. Create and validate `manifest.json` for the bundled frontend/backend outputs; README image inlining does not add manifest entries.
6. Zip the package as `dist/plugin_package.zip`.

If you change any part of this flow, update tests and documentation accordingly.

### README and asset bundling notes

- `README.md` is mandatory; the build will fail if it is missing from the project root.
- Local image references in `README.md` are converted to compressed WebP `data:` URIs during packaging.
- Transformation is applied to image URLs in Markdown image/link nodes, including resolved reference-style uses, and URL-bearing attributes inside raw HTML.
- Local non-image links in markdown and raw HTML are left unchanged.
- External URLs (`http://`, `https://`) trigger a warning message and have their URL removed (set to empty string for markdown nodes, emptied attribute value for HTML) to prevent loading external resources in the plugin package.
- `data:` URIs are preserved as-is because they are self-contained.
- Fragment-only links (e.g., `#purpose`) are preserved as same-document anchors and are not rewritten.
- Rust/WASM decodes and resizes README images within 1600×1600 pixels without enlargement. Rust orchestrates conversion, supplies typed RGBA image metadata and encoder options to a TypeScript callback, and creates base64 data URIs. Rust selects lossy WebP quality starting at 80 with method 6 and retries in steps of 10 down to quality 20; the callback only initializes and invokes `@jsquash/webp`. Inputs are capped at 40 million pixels, encoded images at 100 KiB, and transformed README content at 2 MiB. Images that still exceed the byte limit fail packaging.
- Comrak AST parsing/serialization and html5ever tokenization/serialization handle README transformations; do not replace these libraries with regex-based or custom source-scanning approaches. Preserve rendered Markdown structure and inline HTML boundaries; exact source formatting is not preserved.
- The TypeScript adapter performs filesystem containment checks, reads local assets, and loads the encoder WASM from the installed `@jsquash/webp` dependency using `createRequire.resolve` and filesystem reads (not file-URL fetch). Image decoding/resizing and URL transformation run in Rust/WASM. Tests use checked-in image fixtures, without a PNG generator or test-only image-processing dependency. Focus on README packaging, conversion to smaller WebP output for representative inputs, resizing, size limits, and path/URL handling rather than testing the image crate's codec implementations.

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
- Comrak and html5ever provide robust README parsing and serialization in Rust/WASM; prefer their APIs over custom parsing or source rewriting.
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

1. `mise run build:prod`
2. `mise run typecheck:prod`
3. `mise run test`
4. `mise run format:prod`
5. `mise --env dev run audit:prod`
6. `mise run knip:prod`
7. `mise run lint:prod`

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
