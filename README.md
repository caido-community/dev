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

## Community packaging integration test

On Linux, macOS Apple Silicon, or Windows x64 (Git Bash), run:

```bash
mise --env community install --locked
mise exec --env community -- pnpm install --frozen-lockfile
mise --env community run test-community
```

The shared `.mise/tasks/test-community` task selects a platform script under `scripts/test-community/platforms/`: `linux.sh`, `macos.sh`, or `windows.sh`. Those scripts own timeout prerequisites, executable path conversion, and platform-specific child-process environment variables. Repository discovery and clone/install/build/archive checks remain shared.

This opt-in network test builds this checkout with `build:prod`, fetches the live [plugin store index](https://raw.githubusercontent.com/caido/store/main/plugin_packages.json), and attempts to package every unique `caido-community` repository with this checkout's CLI. It checks the resulting ZIP, README, manifest, and plugin entrypoints. It is separate from pull-request validation.

The community Mise environment pins Node.js 24, pnpm 11.18.0, pnpm 10.20.0 as an install fallback, jq, and ouch. Downloads are locked for Linux x64/ARM64, Windows x64, and macOS ARM64. Bash, curl, Git, find, and GNU timeout are OS prerequisites. On macOS, install coreutils (`brew install coreutils`) and Rosetta (`softwareupdate --install-rosetta --agree-to-license`) for ouch's Intel binary. On Windows, use Git Bash.

Each repository is shallow-cloned into a temporary directory. Failed primary installs get one pnpm 10.20.0 retry after removing workspace `node_modules`, preserving frozen lockfiles and build-script approvals. Automatic package-manager switching is disabled for the retry and subsequent build. Clone, install, build, and decompression each have ten-minute timeouts. All repositories are attempted; failures produce a nonzero exit code and retain temporary clones and per-stage logs at the printed path. Successful runs remove temporary files.

To test one listed repository, run `mise --env community run test-community -- --repository caido-community/authmatrix`.

The **Test community plugins** GitHub Actions workflow automatically runs the full sweep when a pull request changes packaging/build code, manifests/configuration, dependencies, or community-test tooling. It also supports **Actions → Test community plugins → Run workflow**. Runs use disposable Ubuntu, Windows, and macOS runners and are advisory: compatibility failures do not fail the overall workflow. A separate **Report community packaging** workflow posts a PR comment with platform results and a link to repository-level PASS/FAIL output. The reporting workflow must be on the default branch before GitHub can trigger it. It reads only GitHub job metadata, never checks out PR code or downloads artifacts, and keeps its comment-write permission separate from third-party execution. The sweep uses read-only repository permissions and does not persist checkout credentials. Configure the repository Actions variable `CI__MISE_VERSION` for the Mise setup action. Failed sweeps upload per-stage logs and the summary as `community-packaging-logs-<os>` artifacts retained for seven days; cloned repositories, dependencies, and package output are not uploaded.

**Security:** this runs third-party installation scripts and build hooks. Use a disposable environment without credentials or sensitive files, never a privileged self-hosted runner. Child processes receive an allowlisted environment and isolated home, but this is not a sandbox. Results depend on the live store, repository default branches, registries, and network availability.

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

## README Assets

Plugin packages always include the root `README.md`. Local README images are resized when needed and converted to compressed WebP data URIs during packaging, with each image limited to about 100 KiB before base64 encoding and the final README limited to about 2 MiB.

The packaged README is intended for self-contained rendering in Caido. Relative local image references are inlined, relative local non-image links are preserved, and fragment links such as `#purpose` are preserved. Unsupported URL schemes and absolute URLs, including `http:`, `https:`, protocol-relative URLs, `javascript:`, `file:`, `blob:`, and `mailto:`, are removed from Markdown and raw HTML URL attributes. Existing `data:image` URIs are preserved only in image contexts when they are small base64 raster images; other `data:` URLs are removed.
