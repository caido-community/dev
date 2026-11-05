<div align="center">
  <img width="1000" alt="image" src="https://github.com/caido-community/.github/blob/main/content/banner.png?raw=true">

  <br />
  <br />
  <a href="https://github.com/caido-community" target="_blank">Github</a>
  <span>&nbsp;&nbsp;•&nbsp;&nbsp;</span>
  <a href="https://developer.caido.io/" target="_blank">Documentation</a>
  <span>&nbsp;&nbsp;•&nbsp;&nbsp;</span>
  <a href="https://links.caido.io/www-discord" target="_blank">Discord</a>
  <br />
  <hr />
</div>

# 👨‍🏭 Dev

A development toolkit for building Caido plugins. This CLI tool simplifies the process of developing, building, and packaging Caido plugins.

## 🏎️ Installation

```bash
pnpm install -D @caido-community/dev
```

You can then access the binary via `caido-dev`. If you installed it local to your package, it can be run using `pnpm exec caido-dev`.

## Development

The approved native migration uses a small Node.js launcher to select a packaged Rust executable. Node.js 20 or newer is required for **every invocation**, including `--help` and `--version`; installing the npm package does not require Rust. Platform-specific optional npm packages supply binaries for Linux x64, Linux ARM64, Windows x64, and macOS ARM64, with no runtime binary downloads. Keep optional dependencies enabled. Unsupported platforms or missing optional packages must produce actionable errors; macOS x64 and Windows ARM64 are not initial targets.

Rust owns argument parsing, help/version and exit status, build orchestration, package filesystem access, asset validation/recursive collection/reads, containment/symlink checks, README/image processing, package layout, manifest and ZIP assembly, output replacement, and watch mode. A constrained Node adapter evaluates executable `caido.config.ts` files and imports, validates them with the existing Zod schemas, and runs unchanged Vite frontend and tsup backend builds. Arbitrary Vite options and plugins remain within Node. After all plugins build, the adapter expands distinct frontend/backend asset patterns with the existing glob 11.0.1 defaults (`new Glob(pattern, { cwd })`) and returns absolute matched path strings, including directory matches. It does not validate asset filesystem paths, read asset contents, recursively collect matched directories, or copy/package assets; Rust owns those operations. The adapter exchanges versioned JSON metadata over stdio, not preloaded package bytes; stdout is protocol-only and stderr carries diagnostics. `defineConfig` remains the only public JavaScript export, a typed identity helper rather than runtime validation.

Rust checks package destinations for unsafe names, case-insensitive duplicates, and file/directory collisions. Output safety checks run before input reads and again immediately before replacing `dist/`. Config, build, and preflight failures must preserve existing outputs. Containment checks are not race-proof, and rollback after low-level filesystem write failures is platform-dependent. Outputs remain `dist/plugin_package/` and `dist/plugin_package.zip`; workflow plugins remain represented in config but have no build implementation.

Native build and Mise tasks are implemented; final migration validation is in progress. Release packaging stages a universal npm manifest with four exact-version optional binary dependencies; these unpublished packages are not dependencies of the source checkout. Each native package includes `THIRD_PARTY_NOTICES` assembled from the locked Cargo sources, including bundled libwebp license and patent terms. The selected lossy encoder is `webp` 0.3.1 with resolved `libwebp-sys` 0.9.6, conditional on release gates: encoder build/smoke checks on all four targets and a license audit of the resolved dependency graph. Only macOS ARM64 has been validated locally so far. This is not a claim that cross-platform tests or publication are complete; if a gate fails, revisit the dependency decision rather than silently using lossless encoding.

To build from source:

```bash
mise --env dev install --locked
mise --env dev exec -- pnpm install
mise --env dev run build:dev
mise --env dev run test
mise --env dev run format:prod
mise --env dev run lint:prod
mise --env dev run typecheck:prod
mise --env dev run audit:prod
mise --env dev run zizmor
mise --env dev run knip:prod
```

`.mise/config.toml` and `.mise/mise.lock` contain only the shared build tools: Node.js 24, pnpm 11.18.0, Rust 1.99.0 for native builds. `.mise/config.dev.toml` and `.mise/mise.dev.lock` add cargo-deny and zizmor for repository development and CI checks. The `dev` environment is opt-in, not automatically loaded; use `mise --env dev` (or set `MISE_ENV=dev`) for the full development toolset, especially `audit:dev`, `audit:prod`, and `zizmor`. Task suffixes such as `:dev` do not select a Mise environment. Shared tasks such as `mise run test` still work without an environment. The separate `community` environment adds jq, ouch, and a pnpm 10.20.0 fallback to the shared tools, without the dev audit tools. Its multi-version pnpm configuration keeps the shared pnpm 11.18.0 first; pnpm 10 is selected only after an install failure.

pnpm is installed through Mise's standard Aqua backend. Its standalone release is locked for Linux x64, Linux ARM64, Windows x64, and macOS ARM64; pnpm 11.18.0 has no macOS x64 standalone artifact. Refresh only its supported platforms with `mise lock pnpm --platform linux-x64,linux-arm64,windows-x64,macos-arm64`. `pnpm-workspace.yaml` explicitly permits esbuild's required installation script, independently of the Mise backend.

Executable scripts under `.mise/tasks/` define repository tasks (for example, `.mise/tasks/format/dev` is `format:dev`). Other tasks have `:dev` and `:prod` variants: build uses the same release-ready pipeline in both, and typecheck, audit, and knip run the same checks in both. `test` is a single non-watch task that builds with `build:prod` first. Run the native development CLI with `mise run cli:dev -- build [path]` or `mise run cli:dev -- watch [path]`; `cli:prod` builds first and runs the packaged launcher with the same arguments. `mise tasks ls` lists all tasks.

The native Mise task cutover builds the JavaScript launcher/config helper/adapter and a host-native Rust binary; `cli:dev` runs the native development CLI and `cli:prod` builds and runs the packaged launcher. It removes the WASM target, wasm-bindgen tooling, and `dist/wasm/` from the final pipeline. These task changes are being implemented alongside the migration; local task success does not establish the four-target release gates. Rust dependencies use exact direct versions and a committed `rust/Cargo.lock`; builds use `--locked`. `cargo-deny` checks advisories, licenses, duplicate versions, and dependency sources. `mise --env dev run zizmor` audits GitHub Actions workflows with the pinned zizmor release; it is a single check-only task without `:dev` or `:prod` variants. Without a GitHub token, zizmor runs offline and skips online-only audits. Set `GH_TOKEN` to enable those checks locally; validation CI supplies a read-only token to the audit step. Renovate waits at least 14 days before proposing Cargo dependency updates. Until Renovate is active, manually verify that new or updated Rust dependencies, including transitive versions in `rust/Cargo.lock`, were released at least 14 days ago.

Bash task scripts and community platform helpers are checked with `mise --env dev run shellcheck` (ShellCheck 0.11.0) and `mise --env dev run shfmt:prod` (shfmt 3.14.1). Apply two-space Bash formatting with `mise --env dev run shfmt:dev`. These standalone tasks require the dev environment and are not yet wired into CI.

Formatting and linting are separate: `mise run format:dev` writes Oxfmt and Rust formatting, while `mise run lint:dev` applies safe Oxlint fixes. Their `:prod` variants only check files. Oxlint uses the full [Deslop](https://github.com/maharshi365/deslop) recommended rules, including type-aware checks; optional rules are not enabled. Generated output and README/image fixtures are excluded from formatting so their test inputs remain intact. The Oxc development tools require Node.js 20.19+ or 22.12+; Mise supplies Node.js 24, and the published CLI still supports Node.js 20.

### Community packaging integration test

On **Linux**, **macOS (Apple Silicon)**, or **Windows x64 (Git Bash)**, run:

```bash
mise --env community install --locked
mise --env community run test-community
```

The shared `.mise/tasks/test-community` task selects a platform script under `scripts/test-community/platforms/`: `linux.sh`, `macos.sh`, or `windows.sh`. Those scripts own timeout prerequisites, executable path conversion, and platform-specific child-process environment variables. Repository discovery and clone/install/build/archive checks remain shared.

This opt-in network test builds this checkout with `build:prod`, fetches the live [plugin store index](https://raw.githubusercontent.com/caido/store/main/plugin_packages.json) with curl, and uses jq to select **every unique `caido-community` repository** listed there. It is separate from `mise run test` and pull-request CI. The additional jq, ouch, and Aqua pnpm 10.20.0 fallback tools are pinned in `.mise/config.community.toml`, with Linux x64/ARM64, Windows x64, and macOS ARM64 downloads locked in `.mise/mise.community.lock`; ordinary tool installation and CI are unchanged. Bash 3.2 or newer, curl, Git, find, and GNU timeout are OS prerequisites; Node and the primary pnpm come from the default Mise configuration.

On macOS, install GNU timeout with `brew install coreutils`; the task uses `gtimeout`. ouch 0.6.1 only publishes an Intel macOS binary, selected by its Aqua backend, so Apple Silicon also requires Rosetta (`softwareupdate --install-rosetta --agree-to-license`). On Windows, use Git for Windows' Git Bash, which supplies `/usr/bin/timeout` (not Windows' unrelated `timeout.exe`), find, and the POSIX utilities. Native Windows tool paths and the isolated home/temp directories are converted for the appropriate subprocesses.

The Bash task shallow-clones each repository's default branch into a temporary directory outside this checkout, installs dependencies with pnpm (frozen when a pnpm lockfile exists), and invokes this checkout's absolute `dist/cli.js` directly, not the repository's build script or published `caido-dev`. It removes the clone's existing `dist/` first, decompresses the generated ZIP with ouch, and uses jq to verify a nonempty README and manifest plus each manifest plugin's nonempty entrypoint file. Clone, install, build, and decompression each have a ten-minute timeout; fetching the index has a thirty-second timeout. All selected repositories are attempted, failures are reported by stage, and any failure produces a nonzero exit status. Temporary files and per-stage logs are removed on complete success and retained at the printed path on failure.

If the primary install exits nonzero for any reason, the task removes all workspace `node_modules` directories (including pnpm's virtual store and modules metadata) without following symlinks, then retries the install **once with pnpm 10.20.0**. The original frozen/non-frozen lockfile flag is preserved; lockfiles and dependency build-script approvals are not relaxed. The primary output stays in `install.log`; retry output, including the selected version, goes to `install-pnpm10.log`. Console output identifies retries, and a failed retry is reported at the `install-pnpm10` stage. Automatic package-manager version switching is disabled for the retry and subsequent build, and pnpm 10 stays first on that clone's PATH; the next repository starts with the primary again. pnpm 10 has different dependency installation policies from pnpm 11, and installation scripts may execute under either version. Repository package-manager configuration is not rewritten. Each install attempt has its own ten-minute timeout.

The **Test community plugins** GitHub Actions workflow runs the full sweep on disposable Ubuntu and Windows runners (macOS CI is temporarily disabled) via **Actions → Test community plugins → Run workflow**, with fail-fast disabled so every OS reports its result. It uses the locked community Mise environment and read-only repository permissions, without persisting checkout credentials. It automatically runs for pull requests changing packaging/build code, manifests/configuration, dependencies, or community-test tooling. These checks are advisory and do not block merging. A separate **Report community packaging** workflow posts platform results, failed repository names and failure stages, and a link to full output in a PR comment. Each platform job has a 30-minute timeout; timed-out or cancelled jobs may have incomplete failure lists. The reporting workflow must exist on the default branch before GitHub can trigger it; it reads GitHub job metadata and validates repository failure lines from job logs, and never checks out PR code or downloads artifacts. Configure the repository Actions variable `CI__MISE_VERSION` for the Mise setup action. Failed sweeps upload per-stage logs and the summary as `community-packaging-logs-<os>` artifacts retained for seven days; cloned repositories, dependencies, and package output are not uploaded.

For a single-repository reproduction, use `mise --env community run test-community -- --repository caido-community/authmatrix`. The repository must be listed in the live store; omitting the option always tests the full community set. `-- --help` prints usage.

**Security:** installation lifecycle scripts and configuration/build hooks execute third-party code. Run this task only in a disposable environment without credentials or sensitive files. Child processes receive an allowlisted environment and an isolated home, and do not inherit user Git/npm configuration or token variables. This is not a sandbox: repository code still has your filesystem and network access. Results depend on the current store index, default branches, dependency registries, and network availability.

## 📟 CLI Commands

### Build

```bash
caido-dev build [path] [--config <path-to-config>]
```

- **Description**: Build the Caido plugin.
- **Options**:
  - `-c, --config <path>`: Path to the `caido.config.ts` file.

### Watch

```bash
caido-dev watch [path] [--config <path-to-config>]
```

- **Description**: Start the development server and watch for changes.
- **Options**:
  - `-c, --config <path>`: Path to the `caido.config.ts` file.

Watch retains HTTP GET `/plugin_package.zip` on the configured port (default 3000) and WebSocket messages `connected` (`packageId`, `downloadUrl`), `rebuild` (`downloadUrl`), and `error` (`error`). Native watch uses a 200 ms trailing-edge debounce and serializes builds: one active build plus at most one trailing rebuild for events received during it. Valid config changes reload the config and watch roots; invalid changes keep the last valid setup, and failures preserve the last good package. Port changes require restarting watch. Port 0 requests an available OS-assigned port, which is used in download URLs; other ports must be integers from 1 through 65535.

An initial build failure leaves the watcher/server running and the download route returns 404 until the first successful package. Invalid initial config watches the config path and root package metadata for recovery. A port bind failure reports the port and OS reason, exits 1, cleans up resources, and does not choose another port. Disconnected clients are handled safely. Shutdown cancels and reaps the direct Node adapter process and waits for package assembly already in progress before returning; that assembly may finish during shutdown. Custom build hooks can create subprocess descendants that are not covered by direct-child cancellation. SIGINT is supported on every target; SIGTERM handling is supported on Unix.

### Exit statuses

- `0`: success, help, or version.
- `2`: argument/usage errors, including invoking `caido-dev` without a subcommand.
- `1`: config, build, or runtime errors.

Node is still required for help/version because the installed command uses the launcher.

## README Assets

Plugin packages always include the root `README.md`. Local PNG, JPEG, GIF, and WebP images are resized to fit within 1600×1600 pixels without enlargement and converted to lossy WebP data URIs during packaging. Native Rust checks filesystem containment and symlinks, reads assets, decodes and resizes images, invokes the native lossy WebP encoder, selects encoding quality and retries, enforces size limits, and creates base64 data URIs. Neither Markdown processing nor image encoding uses a TypeScript callback. Encoding starts at quality 80 with method 6 and retries at quality 70, 60, 50, 40, 30, and 20 until the image fits. GIFs within 1600×1600 pixels and 100 KiB are inlined unchanged as GIF data URIs, preserving animation. Larger GIFs fall back to first-frame WebP conversion; animated WebP inputs also include only their first frame. SVG, AVIF, TIFF, and other image formats are unsupported and fail with a clear error. Each local image source must be at most 64 MiB (67,108,864 bytes), inclusive; larger files fail packaging before decoding. Rust checks the opened file's size and reads bounded chunks, allowing at most one extra byte to detect growth. Inputs are also limited to 40 million pixels; each encoded image must fit within 100 KiB before base64 encoding, and the final README must fit within 2 MiB. Some images still exceed the limit at quality 20 and fail packaging: simplify or resize those images.

The packaged README is intended for self-contained rendering in Caido. Markdown is parsed and serialized with Comrak in native Rust, so formatting is normalized, reference-style links and images may become inline forms, and unused reference definitions are omitted. Raw HTML is transformed using html5ever tokenization and serialization. Relative local image references are inlined, relative local non-image links are preserved, and fragment links such as `#purpose` are preserved. Unsupported URL schemes and absolute URLs, including `http:`, `https:`, protocol-relative URLs, `javascript:`, `file:`, `blob:`, and `mailto:`, are removed from Markdown and raw HTML URL attributes. Existing `data:image` URIs are preserved unchanged only in image and HTML resource contexts when they contain valid base64 PNG, JPEG (including `image/jpg`), GIF, or WebP images whose declared MIME type matches the decoded format. MIME parameters and case-insensitive headers are supported. URI references are parsed without base-URL resolution or path normalization; Unicode filenames and spaces in local HTML paths are supported. Invalid URI references (including malformed percent escapes and unescaped backslashes) are removed with a warning. Use forward slashes for relative paths; percent-encoded absolute Unix and Windows paths are blocked after decoding. These images are decoded for validation without resizing, must fit within 100 KiB decoded and the existing encoded URI size limit, and must stay within 40 million pixels. Preserved animations have every frame validated. Invalid, oversized, unsupported, or mismatched image data URIs and all `data:` link URLs are removed with a warning.
