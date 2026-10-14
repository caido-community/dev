#!/usr/bin/env node
import { build, watch } from "./commands";
import { wasm } from "./wasm";

try {
  const options = JSON.parse(
    wasm.parse_cli(JSON.stringify(process.argv.slice(2))),
  ) as {
    command: "build" | "watch" | "dev" | "help" | "version";
    path?: string;
    config?: string;
    help?: string;
  };
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
      console.log("1.0.0");
      break;
    case "dev":
      break;
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
