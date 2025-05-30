import path from "path";

import { describe, expect, it } from "vitest";

import { getZipFileContent } from "../../../playgrounds/utils";

describe("build-frontend", () => {
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
