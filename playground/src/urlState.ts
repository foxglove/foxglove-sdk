// note: we assume the wasm module has already been loaded, which is done in index.tsx
import zstd from "@foxglove/wasm-zstd";
import base64 from "@protobufjs/base64";

type UrlState = {
  code: string;
};

const STATE_VERSION = 1;
// wasm-zstd requires decompressedSize as input so we use an arbitrary maximum supported size
const MAX_DECOMPRESSED_SIZE = 5 * 1024 * 1024;

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();

function serializeState(state: UrlState): string {
  const uncompressed = textEncoder.encode(state.code);
  const compressed = zstd.compress(uncompressed);
  return `${STATE_VERSION}:` + base64.encode(compressed, 0, compressed.length);
}

function deserializeState(serialized: string): UrlState | undefined {
  if (serialized.length === 0) {
    return undefined;
  }
  const prefix = `${STATE_VERSION}:`;
  if (!serialized.startsWith(prefix)) {
    throw new Error("Unable to decode URL state, expected prefix not found");
  }
  const encoded = serialized.substring(prefix.length);
  const compressed = new Uint8Array(base64.length(encoded));
  const compressedLen = base64.decode(encoded, compressed, 0);
  if (compressedLen !== compressed.length) {
    throw new Error("Unable to decode URL state, invalid base64 length");
  }
  const decompressed = zstd.decompress(compressed, MAX_DECOMPRESSED_SIZE);
  return {
    code: textDecoder.decode(decompressed),
  };
}

export function getUrlState(): UrlState | undefined {
  try {
    return deserializeState(window.location.hash.substring(1));
  } catch (err) {
    console.warn("Decoding failed:", err);
    return undefined;
  }
}

export function setUrlState(state: UrlState): void {
  history.replaceState(null, "", "#" + serializeState(state));
}
