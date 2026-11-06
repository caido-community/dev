import { expect, it } from "vitest";

import { caidoConfigSchema } from "./types";

const config = {
  id: "example",
  name: "Example",
  description: "Example",
  version: "1.0.0",
  author: { name: "Tester" },
  plugins: [],
};

it("accepts omitted and unsigned 16-bit watch ports", () => {
  for (const watch of [undefined, {}, { port: 0 }, { port: 3000 }, { port: 65535 }]) {
    expect(caidoConfigSchema.safeParse({ ...config, watch }).success).toBe(true);
  }
});

it.each([-1, 1.5, 65536, "3000", null, NaN, Infinity])(
  "reports invalid watch port %s at the config boundary",
  (port) => {
    const result = caidoConfigSchema.safeParse({ ...config, watch: { port } });
    expect(result.success).toBe(false);
    if (!result.success) expect(result.error.issues[0]?.path).toEqual(["watch", "port"]);
  },
);
