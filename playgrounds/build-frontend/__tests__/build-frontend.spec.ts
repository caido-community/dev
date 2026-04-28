import path from "path";

import { describe, expect, it } from "vitest";

import { getZipFileContent } from "../../../playgrounds/utils";

describe("build-frontend", () => {
  it("should have README.md file", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    expect(readmeContent).toContain("Frontend Build Playground");
  });

  it("should transform README image links to GitHub raw URLs", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    expect(readmeContent).toContain("raw.githubusercontent.com");
    expect(readmeContent).toContain("assets/test.txt");
  });

  it("should remove external image URLs", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // External URLs should be removed (set to empty string)
    expect(readmeContent).not.toContain("http://example.com/image.png");
    expect(readmeContent).not.toContain("https://example.com/image.png");
    // data: URIs should be kept as-is (not removed)
    expect(readmeContent).toContain("data:image/png;base64,abc123");
    // Image syntax should still exist but with empty URLs (alt text preserved)
    expect(readmeContent).toContain("![External HTTP]()");
    expect(readmeContent).toContain("![External HTTPS]()");
    expect(readmeContent).toContain(
      "![Data URI](data:image/png;base64,abc123)",
    );
  });

  it("should transform local markdown links to GitHub raw URLs", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // [Local Doc](assets/test.txt) should become a GitHub raw URL
    expect(readmeContent).toMatch(
      /\[Local Doc\]\(https:\/\/raw\.githubusercontent\.com\/.*\/assets\/test\.txt\)/,
    );
  });

  it("should remove external markdown link URLs", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // External link URL should be removed but link text preserved
    expect(readmeContent).not.toContain("https://example.com/docs");
    expect(readmeContent).toContain("[External Link]()");
  });

  it("should preserve fragment-only links", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // Same-document anchors should be preserved as-is
    expect(readmeContent).toContain("[Jump to section](#purpose)");
  });

  it("should transform reference-style definition URLs", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // [ref-image]: assets/test.txt should be transformed to a GitHub raw URL
    expect(readmeContent).toMatch(
      /\[ref-image\]:\s*https:\/\/raw\.githubusercontent\.com\/.*\/assets\/test\.txt/,
    );
    // [ref-link]: https://example.com/docs is external and should be removed
    expect(readmeContent).not.toContain("https://example.com/docs");
  });

  it("should transform local HTML src/href attributes", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // HTML <img src="assets/test.txt"> and <a href="assets/test.txt"> should be transformed
    expect(readmeContent).toMatch(
      /src="https:\/\/raw\.githubusercontent\.com\/.*\/assets\/test\.txt"/,
    );
    expect(readmeContent).toMatch(
      /href="https:\/\/raw\.githubusercontent\.com\/.*\/assets\/test\.txt"/,
    );
  });

  it("should remove external URLs in HTML attributes", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");
    const readmeContent = await getZipFileContent(zipPath, "README.md");
    expect(readmeContent).toBeDefined();
    // External URLs in HTML should be emptied
    expect(readmeContent).not.toContain("https://example.com/image.png");
    expect(readmeContent).not.toContain("https://example.com/docs");
    expect(readmeContent).toContain('src=""');
    expect(readmeContent).toContain('href=""');
  });
  it("should have manifest.json file", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");

    const manifestJsonContent = await getZipFileContent(
      zipPath,
      "manifest.json",
    );

    expect(manifestJsonContent).toEqual(
      JSON.stringify(
        {
          id: "build-frontend",
          name: "Frontend",
          version: "1.0.0",
          description: "Frontend plugin",
          author: {
            name: "John Doe",
            email: "john.doe@example.com",
            url: "https://example.com",
          },
          links: {},
          plugins: [
            {
              id: "frontend",
              kind: "frontend",
              name: "frontend",
              entrypoint: "frontend/index.js",
              style: "frontend/index.css",
              backend: null,
              assets: "frontend/assets",
            },
          ],
        },
        undefined,
        2,
      ),
    );
  });

  it("should have index.js file", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");

    const indexJsContent = (
      await getZipFileContent(zipPath, "frontend/index.js")
    )?.replace(/\s+/g, "");

    const expectedContent = `
      const o = () => {
        console.log("init");
      };
      export {
        o as init
      };
    `.replace(/\s+/g, "");

    expect(indexJsContent).toEqual(expectedContent);
  });

  it("should have index.css file", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");

    const indexCssContent = await getZipFileContent(
      zipPath,
      "frontend/index.css",
    );

    expect(indexCssContent).toEqual("body{background-color:red}\n");
  });

  it("should have assets txt", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");

    const assetContent = await getZipFileContent(
      zipPath,
      "frontend/assets/test.txt",
    );

    expect(assetContent).toEqual("Hello world");
  });

  it("should have assets recursive", async () => {
    const zipPath = path.resolve(__dirname, "../dist/plugin_package.zip");

    const assetContent = await getZipFileContent(
      zipPath,
      "frontend/assets/data/data.txt",
    );

    expect(assetContent).toEqual("My data");
  });
});
