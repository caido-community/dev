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

The npm package includes a Rust WebAssembly module; installing and running it requires Node.js 20 or newer, not a Rust installation. Rust handles command parsing, package layout and manifest assembly, ZIP generation, Markdown/HTML URL rewriting, and README image conversion. The TypeScript host supplies filesystem and network access, TypeScript configuration loading, and Vite/tsup builds.

Packaging is assembled in memory in one Rust/WASM call. Rust checks every file destination, including expanded assets, for unsafe names, case-insensitive duplicates, and file/directory collisions before the host replaces `dist/`. The host expands asset patterns with the existing Node glob library, checks source containment and symbolic links, and writes the returned byte arrays to `dist/plugin_package/` and `dist/plugin_package.zip`; ZIP creation does not reread those outputs. Manifest validation remains a narrow host callback to Caido's published manifest schema. Invalid inputs and missing README files leave existing outputs intact; filesystem write failures after validation can still leave incomplete outputs.

To build from source:

```bash
mise install
mise exec -- pnpm install
mise run build:dev
mise run test
mise run format:prod
mise run lint:prod
mise run typecheck:prod
mise run audit:prod
mise run zizmor
mise run knip:prod
```

`.mise/config.toml` pins the tools (including the WASM target), and executable scripts under `.mise/tasks/` define repository tasks (for example, `.mise/tasks/format/dev` is `format:dev`). `format:dev` autofixes ESLint issues and formats Rust; `format:prod` checks both without modifying files and is used in CI. This replaces the former lint script, so there is no separate lint task. Other tasks have `:dev` and `:prod` variants: build uses the same release-ready pipeline in both, and typecheck, audit, and knip run the same checks in both. `test` is a single non-watch task that builds with `build:prod` first. Run the source CLI with `mise run cli:dev -- build [path]` or `mise run cli:dev -- watch [path]`; `cli:prod` builds first and runs the packaged CLI with the same arguments. `mise tasks ls` lists all tasks.

`mise run build:prod` produces the JavaScript entrypoints plus `dist/wasm/`, which is included in the npm package. Rust dependencies use exact direct versions and a committed `rust/Cargo.lock`; builds use `--locked`. `cargo-deny` checks advisories, licenses, duplicate versions, and dependency sources. `mise run zizmor` audits GitHub Actions workflows with the pinned zizmor release; it is a single check-only task without `:dev` or `:prod` variants. Without a GitHub token, zizmor runs offline and skips online-only audits. Set `GH_TOKEN` to enable those checks locally; validation CI supplies a read-only token to the audit step. Renovate waits at least 14 days before proposing Cargo dependency updates. Until Renovate is active, manually verify that new or updated Rust dependencies, including transitive versions in `rust/Cargo.lock`, were released at least 14 days ago.

Formatting and linting are separate: `mise run format:dev` writes Oxfmt and Rust formatting, while `mise run lint:dev` applies safe Oxlint fixes. Their `:prod` variants only check files. Oxlint uses the full [Deslop](https://github.com/maharshi365/deslop) recommended rules, including type-aware checks; optional rules are not enabled. Generated output and README/image fixtures are excluded from formatting so their test inputs remain intact. The Oxc development tools require Node.js 20.19+ or 22.12+; Mise supplies Node.js 24, and the published CLI still supports Node.js 20.

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

Plugin packages always include the root `README.md`. Local PNG, JPEG, GIF, and WebP images are resized to fit within 1600×1600 pixels without enlargement and converted to lossy WebP data URIs during packaging. Rust/WASM orchestrates local asset conversion, decodes and resizes images, selects encoding quality and retries, enforces size limits, and creates base64 data URIs. A thin TypeScript host checks filesystem containment and symlinks, reads assets, and invokes `@jsquash/webp` using its prebuilt WASM encoder. No Markdown parsing runs in TypeScript. Encoding starts at quality 80 with method 6 and retries at quality 70, 60, 50, 40, 30, and 20 until the image fits. GIFs within 1600×1600 pixels and 100 KiB are inlined unchanged as GIF data URIs, preserving animation. Larger GIFs fall back to first-frame WebP conversion; animated WebP inputs also include only their first frame. SVG, AVIF, TIFF, and other image formats are unsupported and fail with a clear error. Inputs are limited to 40 million pixels; each encoded image must fit within 100 KiB before base64 encoding, and the final README must fit within 2 MiB. Some images still exceed the limit at quality 20 and fail packaging: simplify or resize those images.

The packaged README is intended for self-contained rendering in Caido. Markdown is parsed and serialized with Comrak in Rust/WASM, so formatting is normalized, reference-style links and images may become inline forms, and unused reference definitions are omitted. Raw HTML is transformed using html5ever tokenization and serialization. Relative local image references are inlined, relative local non-image links are preserved, and fragment links such as `#purpose` are preserved. Unsupported URL schemes and absolute URLs, including `http:`, `https:`, protocol-relative URLs, `javascript:`, `file:`, `blob:`, and `mailto:`, are removed from Markdown and raw HTML URL attributes. Existing `data:image` URIs are preserved only in image contexts when they are small base64 raster images; other `data:` URLs are removed.
