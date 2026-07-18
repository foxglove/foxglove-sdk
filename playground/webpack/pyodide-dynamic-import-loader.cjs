// pyodide.mjs uses `await import(url)` with a runtime-computed URL to fetch its own asm module
// and lazily-loaded packages. Its source annotates these with a `/* webpackIgnore: true */` magic
// comment so bundlers leave them as native dynamic imports, but Pyodide's published build (esbuild)
// strips comments during minification, so the published pyodide.mjs no longer carries the hint.
// Without it, webpack tries to statically resolve these expression-based imports itself and fails
// at runtime with "Cannot find module". This loader re-adds the hint before webpack's own parser
// sees the source. It skips import() calls whose argument is already a string literal (e.g.
// `import("node:fs")`), since those are meant to be resolved by webpack.
module.exports = function pyodideDynamicImportLoader(source) {
  return source.replace(/import\((?!["'])/g, "import(/* webpackIgnore: true */ ");
};
