import { Experimental } from "@foxglove/extension";
// @ts-expect-error: types aren't getting picked up for *.wasm
import wasmUrl from "../cpp/target/component.wasm";

export function activate(extensionContext: Experimental.ExtensionContext): void {
  extensionContext.registerDataLoader({
    type: "file",
    wasmUrl,
    supportedFileType: ".xyz",
  });
}
