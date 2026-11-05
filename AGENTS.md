# AGENTS.md

Guidance for AI agents and contributors working in this repository.

## Project overview and migration status

`@caido-community/dev` builds, watches, bundles, and packages Caido plugins. The approved migration moves CLI behavior into a native Rust executable in one coordinated cutover. Implementation, native Mise task updates, and release validation are in progress; do not interpret this guide as evidence that tests or publishing are complete. See `domain_models/migration-plan.md` and ADR 0001 for the accepted design.

The installed `caido-dev` command uses a small Node launcher selecting a platform-specific optional npm binary package. Node.js `>=20` is required for every invocation, including help/version; pnpm `>=9` remains supported. No runtime binary downloads. Initial targets are Linux x64, Linux ARM64, Windows x64, and macOS ARM64, not macOS x64 or Windows ARM64. Missing optional packages and unsupported targets need actionable diagnostics.

Commands remain `caido-dev build [path]` and `caido-dev watch [path]`, with `-c, --config <path>`. Rust owns argument parsing, help/version, diagnostics/status, build orchestration, filesystem operations, README/image processing, package/manifest/ZIP assembly, output replacement, and watch lifecycle. Exit statuses are 0 for success/help/version, 2 for usage errors (including no subcommand), and 1 for config/build/runtime errors.

## Responsibility boundaries and repository layout

- `rust/src/main.rs`, `cli.rs`, `build.rs`, and `adapter.rs` — native entrypoint, arguments, orchestration, and adapter supervision.
- `rust/src/filesystem.rs`, `package.rs`, `package/native.rs`, and `archive.rs` — safe source traversal, package layout, manifest, output replacement, and ZIP.
- `rust/src/readme.rs` and `readme_image.rs` — Comrak/html5ever URL transformations and native image policy/encoding.
- `rust/src/watch.rs` — native watcher, HTTP download route, WebSocket notifications, and shutdown.
- `src/adapter.ts` — constrained executable config/build adapter. `src/config.ts` loads executable TypeScript config/imports via jiti; `src/types.ts` contains existing strict Zod schemas and inferred types.
- `src/build/` — unchanged Vite frontend and tsup backend behavior. Preserve arbitrary Vite options, functions, plugins, imports, cwd, and module resolution; do not translate them into Rust config.
- `src/index.ts` — `defineConfig`, the only public JavaScript export. It is a typed identity helper; runtime validation happens when the CLI loads config.
- `playgrounds/` — integration fixtures/tests; `.mise/tasks/` — task scripts; `.github/workflows/` — CI/publishing.

The launcher only selects/invokes the native binary. The Node adapter evaluates/validates config, runs Vite/tsup, and performs asset glob pattern expansion only; it returns validated serializable config/build metadata and absolute matched source paths through one-shot versioned JSON over stdio. Preserve glob 11.0.1 `new Glob(pattern, { cwd })` defaults exactly, including directory matches. Expand distinct patterns from built frontend/backend output assets only after all builds finish. Every response includes `assetMatches: Record<string, string[]>`; load returns `{}`. Node does not validate asset filesystem paths, read asset contents, recursively collect matched directories, or copy/package assets; Rust owns those operations. Stdout is protocol-only and stderr carries diagnostics. Rust validates protocol/safety invariants. Do not return preloaded package bytes or add filesystem/README/image/package callbacks to Node. Legacy JS CLI, package host, and WASM files present during implementation are removal targets, not the final responsibility boundary.

Workflow plugins remain represented in config but have no build implementation; do not expand that scope.

## Development environment and commands

Use pnpm for package operations and Mise for repository tasks. `.mise/config.toml` and `.mise/mise.lock` pin Node.js 24, pnpm 11.18.0, and Rust 1.99.0 for native builds. The final pipeline does not require a WASM target or wasm-bindgen CLI. `.mise/config.dev.toml` and its lock add audit tools; `community` separately adds jq, ouch, and pnpm 10.20.0 fallback tooling. Environments are opt-in: `mise --env dev` or `MISE_ENV=dev`; a task suffix such as `:dev` does not activate an environment.

Native task changes are being implemented alongside this migration. Keep these interfaces aligned with their scripts rather than documenting old TypeScript/WASM execution:

- `mise --env dev install --locked` — install shared native tools and dev audit tools.
- `mise exec -- pnpm install` — install dependencies.
- `mise run build:dev` / `build:prod` — build JS launcher/helper/adapter and host-native Rust artifacts, not `dist/wasm/`.
- `mise run cli:dev -- build [path]` / `watch [path]` — run native development CLI.
- `mise run cli:prod -- build [path]` — build and run the packaged launcher.
- `mise run typecheck:prod` — TypeScript checks; native Rust tests must also be included in validation.
- `mise run test` — non-watch test task, dependent on `build:prod`; preserve playground coverage and add native CLI/package/watch coverage.
- `mise run format:dev` / `format:prod` — write/check Oxfmt and Rust formatting.
- `mise run lint:dev` / `lint:prod` — safe Oxlint fixes/checks, including full Deslop recommended type-aware rules. Keep Oxlint aligned with Deslop's exact `@oxlint/plugins` version; optional Deslop rules are not enabled.
- `mise --env dev run audit:prod` — cargo-deny checks of resolved dependencies.
- `mise --env dev run zizmor` — check-only GitHub Actions audit; set `GH_TOKEN` for online checks.
- `mise run knip:prod` — unused code/dependency checks.
- `mise --env dev run shellcheck` and `shfmt:prod` — Bash checks; `shfmt:dev` writes formatting.

Mise supplies Node 24 for development; Oxc tools need Node 20.19+ or 22.12+. Do not raise published runtime engines accidentally. Do not start long-running watch/dev processes unless requested; use a timeout if testing them.

### Community test

`mise --env community install --locked` and `mise --env community run test-community` run the opt-in live-store packaging sweep on Linux, macOS ARM64 (coreutils and Rosetta required), or Windows x64 (Git Bash required). Use `-- --repository caido-community/name` for one listed repository. The task builds this checkout and verifies extracted archives using curl/jq/Git/pnpm/ouch. It retries a failed install once with pnpm 10.20.0 after removing workspace `node_modules`, preserving frozen lockfiles and build-script approvals. Logs are `install.log` and `install-pnpm10.log`; failures retain clones/logs. Both versions may execute third-party lifecycle/build code. Run only in a disposable environment without credentials; environment isolation is not a sandbox. See README for details.

## Build and filesystem invariants

1. Rust invokes the adapter to evaluate/validate executable `caido.config.ts` and orchestrates unchanged Vite/tsup builds.
2. Node expands asset patterns to matched paths only. Rust validates those paths, recursively collects matched directories, and reads assets/build outputs/README after containment and leaf/parent symlink checks. Preserve output checks before reads and immediately before replacing `dist/`. Deduplicate source reads, not destination collision checks.
3. Rust alone plans package destinations, checks unsafe names, case-insensitive duplicates and file/directory collisions, and assembles `manifest.json`. README image inlining adds no manifest entries. Upfront Zod config validation remains required; do not duplicate post-assembly manifest validation in Node.
4. Outputs remain `dist/plugin_package/` and `dist/plugin_package.zip`. Config/build/preflight failures must preserve existing outputs. Use temporary/atomic replacement where feasible and verify each OS; low-level write-failure rollback is platform-dependent. Containment checks are not race-proof.

Use Rust filesystem/path APIs for native operations and Node path utilities only within the adapter/build ecosystem. Preserve cross-platform glob semantics, tool paths and forward-slash ZIP entry names; never hardcode host separators into logic. Test symlink rejection and sentinel preservation on supported platforms.

## README and image policy

The root `README.md` is mandatory and not configurable. Preserve Comrak AST parsing/serialization and html5ever tokenization/serialization; do not replace them with regex or custom scanning. Formatting normalizes, reference uses can become inline forms, and unused definitions disappear. Preserve rendered Markdown and inline HTML boundaries.

Follow the precise README Assets policy in `README.md`, not a blanket allowance for self-contained URLs:

- Relative local non-image links and same-document fragments remain unchanged. Local PNG/JPEG/GIF/WebP image references are inlined; unsupported formats fail clearly.
- Unsupported schemes/absolute URLs (including HTTP(S), protocol-relative, javascript, file, blob, mailto) are removed with warnings. Invalid URI references, malformed escapes and unescaped backslashes are removed; block percent-decoded absolute Unix/Windows paths.
- Existing image data URIs survive **only in image and HTML resource contexts**, with valid base64 PNG/JPEG (including `image/jpg`)/GIF/WebP content matching declared MIME. Support MIME parameters and case-insensitive headers. Validate decoded images without resizing, every preserved animation frame, 100 KiB decoded content, the existing encoded URI size limit, and 40 MP. Invalid, oversized, unsupported, mismatched image data and all `data:` link URLs are removed with warnings. Do not broaden this to arbitrary data content.
- Rust reads local image sources with same-handle stat and bounded chunk reads, closing the handle on success/failure. Source limit is 64 MiB (67,108,864 bytes), inclusive; allow at most one extra byte to detect growth.
- Decode/resize within 1600×1600 without enlargement, enforce 40 million pixels, 100 KiB encoded image size before base64 and 2 MiB final README. Qualifying GIFs within dimensions/100 KiB stay unchanged to preserve animation; larger GIFs and animated WebP use first-frame WebP conversion.
- Native lossy encoding uses quality 80, method 6, lossless disabled, retrying by 10 down to 20. Fail if still oversized. Encoded bytes need not match the former encoder.

`webp` 0.3.1 with resolved `libwebp-sys` 0.9.6 is selected conditionally. All four platform encoder build/smoke checks **and a license audit of the resolved graph gate publishing**. Only macOS ARM64 has been validated locally so far. Do not claim other targets or the license gate passed without evidence. Stop and reopen the dependency decision if a gate fails; no silent lossless fallback.

Use checked-in image fixtures, not a PNG generator or test-only image-processing dependency. Test README presence in ZIP, conversions/size/resizing, valid versus invalid image data contexts, URL removal, paths and limits rather than codec internals.

## Watch contract

Preserve HTTP GET `/plugin_package.zip`, default/configured port (default 3000), and JSON fields: `connected` (`packageId`, `downloadUrl`), `rebuild` (`downloadUrl`), `error` (`error`).

Use a 200 ms trailing-edge debounce; serialize builds with one active and at most one trailing rebuild for events during it. Reload valid config/watch roots live; retain last valid setup on invalid reload and last good package on failure. Port changes require restart. Initial build failure keeps watcher/server available and serves 404 until first package. Invalid initial config watches the config path and root package metadata for recovery. Port bind failure reports port/OS reason, exits 1, cleans up, and never selects a fallback port. Remove disconnected clients safely and gracefully close watcher/WebSocket/HTTP/build resources on SIGINT/SIGTERM.

## Change and validation checklist

Use strict TypeScript/ESM and explicit public types. Keep `src/types.ts` Zod schemas/types, adapter metadata and Rust protocol aligned; preserve clear validation errors. Update config fixtures, manifest assertions, playground/native tests and README for public changes. Keep modules focused and diagnostics consistent.

Use exact direct Rust versions and committed `rust/Cargo.lock`; builds use `--locked`. Never hand-edit lockfiles. Run cargo-deny for advisories/licenses/duplicates/sources. Verify new Rust dependencies, including resolved transitive versions, are at least 14 days old until Renovate enforces the wait. Prefer parser/library APIs over custom parsing and avoid small unnecessary dependencies.

Before finishing a non-trivial change, try `mise run build:prod`, `typecheck:prod`, `test`, `format:prod`, `lint:prod`, `knip:prod`, `mise --env dev run audit:prod`, and `mise --env dev run zizmor`. Report precisely what ran and why any check could not run. A local pass is not the four-target release gate. Cover CLI 0/2/1 statuses, adapter failure propagation, native package/README safety, watch failures/reload/shutdown, optional package installation, and each target's encoder before release.

Never commit credentials, tokens, generated `dist/`, playground `node_modules/`, coverage or temporary packages unless explicitly intended. Treat config/imports/build hooks as executable user code. Avoid deletion outside intended output directories and constrain ZIP entries to the package layout. Document public behavior, cross-platform risks, pending checks, and dependency/lockfile changes honestly.
