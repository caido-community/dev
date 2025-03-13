import { defaultConfig } from "@caido/eslint-config";

export default [
  ...defaultConfig({
    vue: false,
    stylistic: false,
    compat: false,
    node: true,
  }),
];
