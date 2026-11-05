#!/usr/bin/env node
import { version } from "../package.json";

import { build, watch } from "./commands";
import { wasm } from "./wasm";

try {
  const options = wasm.parse_cli(process.argv.slice(2));
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
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
