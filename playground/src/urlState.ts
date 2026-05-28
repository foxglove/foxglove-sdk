// note: we assume the wasm module has already been loaded, which is done in index.tsx
import zstd from "@foxglove/wasm-zstd";

export type UrlState = {
  code: string;
  layout?: unknown;
};

// https://developer.mozilla.org/en-US/docs/Glossary/Base64#url_and_filename_safe_base64
function base64ToUrlSafe(b64: string): string {
  return b64.replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}
function urlSafeToBase64(urlSafe: string): string {
  return urlSafe.replaceAll("-", "+").replaceAll("_", "/");
}

const STATE_VERSION = 1;
// wasm-zstd requires decompressedSize as input so we use an arbitrary maximum supported size
const MAX_DECOMPRESSED_SIZE = 5 * 1024 * 1024;

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();

/** Apply zstd + base64 to compress a string */
function compressEncode(value: string | undefined): string {
  if (!value) {
    return "";
  }
  const uncompressed = textEncoder.encode(value);
  const compressed = zstd.compress(uncompressed);
  return base64ToUrlSafe(compressed.toString("base64"));
}

/** Decode base64 and decompress zstd */
function uncompressDecode(encodedUrlSafe: string | undefined): string {
  if (!encodedUrlSafe) {
    return "";
  }
  const compressed = Buffer.from(urlSafeToBase64(encodedUrlSafe), "base64");
  if (compressed.length === 0) {
    return "";
  }
  const decompressed = zstd.decompress(compressed, MAX_DECOMPRESSED_SIZE);
  return textDecoder.decode(decompressed);
}

function serializeState(state: UrlState): string {
  const params = new URLSearchParams({
    v: STATE_VERSION.toString(),
    code: compressEncode(state.code),
  });
  if (state.layout != undefined) {
    params.set("layout", compressEncode(JSON.stringify(state.layout)));
  }
  return params.toString();
}

function deserializeState(serialized: string): UrlState | undefined {
  if (serialized.length === 0) {
    return undefined;
  }
  const params = new URLSearchParams(serialized);
  const version = params.get("v");
  if (!version) {
    throw new Error("Unable to decode URL state: missing version");
  } else if (version !== STATE_VERSION.toString()) {
    throw new Error(`Unable to decode URL state: missing version ${params.get("v") ?? ""}`);
  }
  const encodedCode = params.get("code") ?? "";
  const encodedLayout = params.get("layout") ?? "";
  const code = uncompressDecode(encodedCode);
  const layoutJson = uncompressDecode(encodedLayout);
  const layout = layoutJson ? (JSON.parse(layoutJson) as unknown) : undefined;
  return { code, layout };
}

export function getUrlState(): UrlState | undefined {
  return deserializeState(window.location.hash.substring(1));
}

export function setUrlState(state: UrlState): void {
  history.replaceState(null, "", "#" + serializeState(state));
}
