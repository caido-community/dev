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
