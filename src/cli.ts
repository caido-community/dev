#!/usr/bin/env node
import { z } from "zod";

import { version } from "../package.json";

import { build, watch } from "./commands";
import { wasm } from "./wasm";

try {
  const options = z
    .object({
      command: z.enum(["build", "watch", "dev", "help", "version"]),
      path: z.string().optional(),
      config: z.string().optional(),
      help: z.string().optional(),
    })
    .parse(JSON.parse(wasm.parse_cli(JSON.stringify(process.argv.slice(2)))));
  switch (options.command) {
    case "build":
      await build(options);
      break;
    case "watch":
      await watch(options);
      break;
    case "help":
      console.log(options.help);
      break;
    case "version":
      console.log(version);
      break;
    case "dev":
      break;
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
