# Context

## Agreed terminology

- **Native CLI:** the compiled Rust `caido-dev` executable; it owns command parsing, diagnostics, exit status, orchestration, packaging, and watch-server lifecycle.
- **Node build adapter:** a Node process invoked by the native CLI only for executable config evaluation/validation and existing Vite/tsup builds.
- **Build ecosystem:** existing Vite frontend and tsup backend behavior, including imported TypeScript config and arbitrary Vite options/plugins.
- **Package output:** `dist/plugin_package/` and `dist/plugin_package.zip`, including manifest and transformed README.
- **WASM path:** current Rust-to-Node WASM-bindgen interface; transitional and not part of the selected final architecture.

## Pre-migration baseline

These observations record the design baseline; implementation is now approved and changing the checkout in a single cutover. They are not final ownership guidance.

- The published npm package currently exposes `dist/cli.js`, declares Node.js `>=20` and pnpm `>=9`, and publishes through one Ubuntu workflow.
- Rust/WASM handles clap argument parsing, package/manifest layout and checks, ZIP creation, Comrak Markdown transformation, html5ever HTML token processing, and much of README image policy.
- TypeScript evaluates configs with jiti, validates with strict Zod schemas, runs Vite/tsup, expands asset globs, performs containment/symlink checks, writes package outputs, and owns watch mode with HTTP downloads and WebSocket status messages.
- `defineConfig` is the only public JS export.
- Workflow plugins are represented in config but have no build implementation; this migration does not expand workflow build scope.
- Current watch mode builds independently on every event, does not reload config, and has no explicit shutdown lifecycle.

## Agreed architecture and behavior

- The installed `caido-dev` command uses a small Node.js launcher to select and invoke the native Rust binary from platform-specific optional npm packages. Node is required for every CLI invocation, including help/version, and remains required for build/watch adapter work.
- npm remains the installation channel; platform-specific optional binary packages are selected by the launcher. The one-line npm installation flow is retained.
- Initial targets: Linux x64, Linux ARM64, Windows x64, macOS ARM64. macOS x64 and Windows ARM64 are not promised initially.
- Native-only Rust is the final architecture. The WASM-bindgen interface and JS encoder callback are removed at cutover.
- Node evaluates executable config, applies existing Zod validation, runs Vite/tsup, and returns build metadata. `defineConfig` remains exported. Rust validates protocol/safety invariants.
- Node retains glob 11.0.1 pattern expansion only, returning absolute matched paths after builds; Rust owns package filesystem access, matched-path validation, recursive directory collection and asset reads, containment/symlink checks, README processing, package assembly, output replacement, and watch lifecycle.
- Adapter protocol is one-shot versioned JSON over stdio; stdout is protocol-only and stderr is diagnostic output.
- README preserves supported formats, 1600×1600 resize ceiling, GIF/WebP frame rules, quality retry policy, 64 MiB source / 40 MP / 100 KiB image / 2 MiB README bounds, URL transformations, and Comrak/html5ever parsing. WebP bytes may differ from JSquash.
- `webp` 0.3.1 with resolved `libwebp-sys` 0.9.6 is selected conditionally. All four target encoder build/smoke checks and a license audit of the resolved dependency graph gate publishing. Only macOS ARM64 has been validated locally so far; otherwise checks remain pending. Stop and revisit on gate failure, with no silent lossless fallback.
- Watch filesystem events use a 200 ms trailing-edge debounce; builds are serialized with at most one trailing rebuild for changes during an active build. Valid config/root changes reload live, invalid config keeps the last valid setup, failed builds retain the last successful package, disconnected clients are safely removed, and shutdown is graceful. Initial build failure keeps watch/server alive; invalid initial config watches the config path for recovery. Port changes require restarting watch. Port bind failure exits with status 1, actionable diagnostic, and cleanup.
- Exit codes: 0 for success/help/version; 2 for argument/usage errors; 1 for config/build/runtime failures.
- Migration is approved for implementation as a single cutover. Native Mise build/CLI/test/task updates are being implemented alongside the runtime; tests and release gates are not declared complete.

## Remaining validation risks

- Verify npm installation and the approved small Node launcher select the correct optional native binary, without runtime downloads, and handle missing optional packages/unsupported targets clearly. Node is required even for help/version.
- Prove native encoder builds and behaves acceptably on all four targets; compare visual/output size behavior, not bytes.
- Define exact adapter operations/schema and diagnostic wording in implementation tests, without changing the accepted boundary.
- Filesystem containment checks are not race-proof; preserve protections and document that limit. Preflight/config/build failures preserve existing `dist/`; atomic swap/rollback after low-level write failures is platform-dependent and must be verified.
