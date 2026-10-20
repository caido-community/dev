import type { WebPModule } from "@jsquash/webp/codec/enc/webp_enc.js";
import "@jsquash/webp/encode.js";

// @jsquash/webp 1.5.0 accepts a compiled module but omits this overload.
declare module "@jsquash/webp/encode.js" {
  function init(module: WebAssembly.Module): Promise<WebPModule>;
}
