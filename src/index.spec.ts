import { expect, it } from "vitest";

import { defineConfig } from "./index";

it("exports only the typed configuration identity helper", async () => {
  const config = {
    id: "example",
    name: "Example",
    description: "Example",
    version: "1.0.0",
    author: { name: "Tester" },
    plugins: [],
  };
  expect(defineConfig(config)).toBe(config);
  expect(Object.keys(await import("./index"))).toEqual(["defineConfig"]);
});
