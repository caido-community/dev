import { describe, it, expect } from 'vitest';

import { getZipFileContent } from '../../utils';
import path from 'path';

describe('build-backend', () => {
  it("should have manifest.json file", async () => {
    const zipPath = path.resolve(__dirname, '../dist/plugin_package.zip');

    const manifestJsonContent = await getZipFileContent(zipPath, 'manifest.json');

    expect(manifestJsonContent).toEqual(JSON.stringify({
      "id": "build-backend",
      "name": "Backend",
      "version": "1.0.0",
      "description": "Backend plugin",
      "author": {
        "name": "John Doe",
        "email": "john.doe@example.com",
        "url": "https://example.com"
      },
      "plugins": [
        {
          "id": "backend",
          "kind": "backend",
          "name": "backend",
          "entrypoint": "backend/index.js",
          "runtime": "javascript"
        }
      ]
    }, undefined, 2));
  });

  it("should have index.js file", async () => {
    const zipPath = path.resolve(__dirname, '../dist/plugin_package.zip');

    const indexJsContent = (await getZipFileContent(zipPath, 'backend/index.js'))?.replace(/\s+/g, '');

    const expectedContent = `
      //packages/backend/src/index.ts
      function init() {
      }
      export {
        init
      };
    `.replace(/\s+/g, '');

    expect(indexJsContent).toEqual(expectedContent);
  });
});
