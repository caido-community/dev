# ADR 0001: Native Rust CLI with a Node build adapter

- Status: Accepted
- Date: 2026-11-05

## Decision

The published npm package exposes `caido-dev` through a small Node.js launcher that selects and invokes a native Rust binary from platform-specific optional packages. npm remains the install channel and the existing one-line install workflow. This revises the earlier no-Node-launch requirement: Node is required to invoke every CLI command, including `--help` and `--version`. The native executable owns the actual CLI behavior. The initial targets are Linux x64, Linux ARM64, Windows x64, and macOS ARM64. macOS x64 and Windows ARM64 are not promised initially.

Node.js is also required for plugin builds because config/build operations use the adapter. A constrained Node adapter evaluates executable `caido.config.ts` files and imports, applies the existing Zod schema, and runs existing Vite frontend and tsup backend builds without translating or restricting their capabilities. The adapter returns validated serializable config/build metadata; arbitrary executable Vite values remain within Node. The approved boundary revision retains only asset pattern expansion in Node using the existing glob 11.0.1 `new Glob(pattern, { cwd })` defaults. After all builds finish, it deduplicates patterns from built frontend/backend outputs and returns absolute matched source paths, including directory matches. Node performs no asset filesystem validation, content reads, recursive directory collection, copying, or packaging. Rust owns those operations. `defineConfig` remains the only public JS export.

The adapter uses one-shot, versioned JSON over stdio. Stdout is protocol-only; stderr carries diagnostics; process failures are propagated. Rust owns argument parsing/help/version, orchestration, package filesystem operations, asset path validation/recursive collection/reads, containment/symlink checks, README handling, package/manifest/ZIP assembly, output replacement, and watch lifecycle. The final Rust implementation is native-only; current WASM-bindgen APIs and JS callbacks are transitional and removed at cutover.

Comrak and html5ever remain the README parsers/serializers. README image policy retains supported formats, resize/frame behavior, quality retries, and documented size limits, but encoded WebP bytes need not match JSQuash. `webp` 0.3.1 with resolved `libwebp-sys` 0.9.6 is selected conditionally. Encoder build/encoding smoke tests on all four targets and a license audit of the resolved dependency graph gate publishing. Only macOS ARM64 has been validated locally so far. A gate failure reopens the dependency decision, with no silent switch to lossless output.

Watch builds are coalesced and serialized. Valid config/root changes reload live; invalid config retains the last valid setup; failures preserve the last successful package; HTTP/WebSocket protocol fields remain compatible; disconnected clients are handled safely; shutdown closes resources gracefully. Port changes require restarting watch.

Exit codes: 0 for success/help/version, 2 for usage/argument errors, and 1 for config/build/runtime errors. This changes current malformed-argument status from 1 to 2.

The migration is a single cutover. Workflow plugins remain unbuilt and out of scope.

## Rationale

- Rust owns the executable process and native filesystem/package/watch responsibilities, satisfying the requested migration meaningfully.
- Executable TypeScript configs and arbitrary Vite options/plugins require Node; keeping a narrow adapter preserves the current build ecosystem instead of silently restricting it.
- npm preserves the current installation channel; optional target packages avoid runtime binary downloads. The small Node launcher reliably exposes the root npm command while Rust owns actual CLI behavior; Node remains required to launch.
- A native lossy encoder removes the WASM callback while preserving policy. Byte-identical image output is not a documented contract.

## Alternatives considered

- Keep Node CLI and move more logic into Rust/WASM: rejected because JavaScript remains the primary CLI process.
- Node-free builds replacing/restricting TypeScript config, Vite, and tsup: rejected as incompatible with current capabilities.
- Direct native command with no launcher: rejected after npm/pnpm documentation and native-package patterns did not establish that nested optional-package bins are promoted into the root command.
- Install-time binary downloads or separate binary distribution: rejected in favor of npm optional packages and the existing npm-first workflow.
- Shared Rust core plus native and WASM adapters: rejected; final architecture is native-only with a single cutover.
- Byte-identical WebP output: rejected; preserve policy and validate size/visual behavior.
- `image-webp` or `image` built-in WebP encoder as lossy replacement: rejected by docs because they support lossless-only output. `webp`/`libwebp-sys` remains conditional on target and legal/build validation.

## Consequences and risks

- Node is a runtime dependency for all CLI invocation via the launcher, as well as build/watch adapter operations; docs/engines must state this accurately.
- The launcher must fail clearly when no supported optional binary is installed or optional dependencies were omitted.
- Four targets need native build, install, encoder, CLI, and protocol smoke tests; unsupported targets need actionable errors.
- Adapter process/module resolution, cwd semantics, stdout/stderr separation, and failure propagation need explicit tests.
- Filesystem protections must preserve current leaf/parent symlink and output replacement checks; they cannot guarantee race-free containment.
- Update README, AGENTS, Mise tasks, CI/publishing, package metadata, lockfiles, and remove stale WASM guidance at cutover.
