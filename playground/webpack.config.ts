import ReactRefreshPlugin from "@pmmmwh/react-refresh-webpack-plugin";
import { PyodidePlugin } from "@pyodide/webpack-plugin";
import CopyWebpackPlugin from "copy-webpack-plugin";
import HtmlWebpackPlugin from "html-webpack-plugin";
import MonacoWebpackPlugin from "monaco-editor-webpack-plugin";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { version as pyodideVersion } from "pyodide";
import reactRefreshTypescript from "react-refresh-typescript";
import webpack, { Compiler, Configuration } from "webpack";

const thisDirname = path.dirname(fileURLToPath(import.meta.url));

type WebpackArgv = {
  mode?: string;
};

const wheelPath = fs.globSync("public/foxglove_sdk-*.whl", { cwd: thisDirname })[0];
if (!wheelPath) {
  throw new Error("Expected a foxglove_sdk .whl file in the public directory");
}

export default (_env: unknown, argv: WebpackArgv): Configuration => {
  const isDev = argv.mode !== "production";
  const allowUnusedVariables = isDev;
  return {
    entry: "./src/index",
    target: "web",
    // Pyodide's runtime (pyodide.asm.mjs) is an ES module and refuses to run in a classic worker,
    // so RunnerWorker must be a module-type worker. Webpack can only emit module-type worker
    // chunks if the whole build output is ESM (see
    // https://github.com/pyodide/pyodide-webpack-plugin/blob/main/examples/esm/webpack.config.js).
    experiments: {
      outputModule: true,
    },
    output: {
      filename: "index.js",
      path: path.resolve(thisDirname, "dist"),
      chunkFormat: "module",
      library: {
        type: "module",
      },
    },
    devtool: argv.mode === "production" ? false : "eval-source-map",
    module: {
      rules: [
        {
          test: /\.tsx?$/,
          exclude: /node_modules/,
          use: {
            loader: "ts-loader",
            options: {
              getCustomTransformers: () => ({
                before: isDev ? [reactRefreshTypescript()] : [],
              }),
              compilerOptions: {
                noUnusedLocals: !allowUnusedVariables,
                noUnusedParameters: !allowUnusedVariables,
              },
            },
          },
        },
        {
          test: /\.wasm$/,
          type: "asset/resource",
        },
        {
          test: /\.ttf$/,
          type: "asset/resource",
        },
        {
          test: /\.css$/,
          use: ["style-loader", "css-loader"],
          sideEffects: true,
        },
        {
          // pyodide.mjs uses `await import(url)` with a runtime-computed URL to fetch its own
          // asm module and lazily-loaded packages. Its source annotates these with a
          // `/* webpackIgnore: true */` magic comment so bundlers leave them as native dynamic
          // imports, but Pyodide's published build (esbuild) strips comments during
          // minification, so the published pyodide.mjs no longer carries the hint. Without it,
          // webpack tries to statically resolve these expression-based imports itself and fails
          // at runtime with "Cannot find module". This loader re-adds the hint before webpack's
          // own parser sees the source.
          test: /pyodide\.mjs$/,
          loader: path.resolve(thisDirname, "webpack/pyodide-dynamic-import-loader.cjs"),
        },
      ],
    },
    resolve: {
      extensions: [".tsx", ".ts", ".js"],
      fallback: {
        // pyodide.mjs conditionally imports these Node builtins for its non-browser code path;
        // they're unreachable at runtime in the browser, but webpack still needs to resolve them.
        fs: false,
        "fs/promises": false,
        path: false,
        url: false,
        vm: false,
      },
    },
    plugins: [
      // resolve.fallback doesn't match "node:"-prefixed specifiers (webpack/webpack#14166), so
      // strip the prefix first and let the bare-name fallbacks above handle it.
      new webpack.NormalModuleReplacementPlugin(/^node:/, (resource) => {
        resource.request = resource.request.replace(/^node:/, "");
      }),
      new webpack.ProvidePlugin({
        Buffer: ["buffer", "Buffer"],
      }),
      new webpack.DefinePlugin({
        FOXGLOVE_SDK_WHEEL_FILENAME: JSON.stringify(path.basename(wheelPath)),
      }),
      new CopyWebpackPlugin({
        patterns: [{ from: path.resolve(thisDirname, "public") }],
      }),
      new HtmlWebpackPlugin({
        scriptLoading: "module",
        templateContent: /* html */ `
<!doctype html>
<html>
  <head>
    <title>Foxglove SDK Playground</title>
    <meta name="description" content="Learn to use the Foxglove SDK to visualize data in a playground environment."/>
    <meta property="og:title" content="Foxglove SDK Playground"/>
    <meta property="og:description" content="Learn to use the Foxglove SDK to visualize data in a playground environment."/>
    <meta property="og:type" content="website"/>
  </head>
  <body>
    <div id="root"></div>
  </body>
</html>
`,
      }),
      new PyodidePlugin(),
      new MonacoWebpackPlugin(),
      isDev &&
        new ReactRefreshPlugin({
          // Don't duplicate webpack dev server overlay
          overlay: false,
        }),
      new PyodideCdnDownloadPlugin({
        pyodideCdnPackages: [
          // Pyodide is distributed with a list of packages that it knows about. These filenames match
          // the ones it will try to download at runtime when calling pyodide.loadPackage(). See the
          // list at: https://pyodide.org/en/stable/usage/packages-in-pyodide.html
          "micropip-0.11.1-py3-none-any.whl",
          "numpy-2.4.3-cp314-cp314-pyemscripten_2026_0_wasm32.whl",
          "libopenblas-0.3.28.zip",
          "opencv_python-4.11.0.86-cp314-cp314-pyemscripten_2026_0_wasm32.whl",
          "packaging-26.1-py3-none-any.whl",
          "pandas-3.0.2-cp314-cp314-pyemscripten_2026_0_wasm32.whl",
          "parso-0.8.6-py2.py3-none-any.whl",
          "protobuf-7.34.1-cp314-cp314-pyemscripten_2026_0_wasm32.whl",
          "python_dateutil-2.9.0.post0-py2.py3-none-any.whl",
          "pytz-2026.1.post1-py2.py3-none-any.whl",
          "scipy-1.18.0-cp314-cp314-pyemscripten_2026_0_wasm32.whl",
          "six-1.17.0-py2.py3-none-any.whl",
        ],
        pypiPackageUrls: [
          // upgraded version of jedi to fix https://github.com/davidhalter/jedi/issues/2087 and https://github.com/davidhalter/jedi/issues/2073
          "https://files.pythonhosted.org/packages/9a/93/242e2eab5fe682ffcb8b0084bde703a41d51e17ee0f3a31ff0d9d813620a/jedi-0.20.0-py2.py3-none-any.whl",
        ],
      }),
    ],
  };
};

/**
 * Download python wheel files from Pyodide's CDN at build time
 *
 * See available packages at: https://pyodide.org/en/stable/usage/packages-in-pyodide.html
 */
class PyodideCdnDownloadPlugin {
  #assets: Promise<Array<{ name: string; data: Buffer }>>;

  constructor(params: { pyodideCdnPackages: string[]; pypiPackageUrls: string[] }) {
    const assets = params.pyodideCdnPackages
      .map((name) => ({
        name,
        url: `https://cdn.jsdelivr.net/pyodide/v${pyodideVersion}/full/${name}`,
      }))
      .concat(params.pypiPackageUrls.map((url) => ({ name: url.split("/").at(-1)!, url })));

    this.#assets = Promise.all(
      assets.map(async ({ name, url }) => {
        console.log("fetching", url);
        const data = await (await fetch(url)).arrayBuffer();
        return { name, data: Buffer.from(data) };
      }),
    );
  }
  apply(compiler: Compiler): void {
    compiler.hooks.thisCompilation.tap(PyodideCdnDownloadPlugin.name, (compilation) => {
      compilation.hooks.processAssets.tapPromise(
        {
          name: PyodideCdnDownloadPlugin.name,
          stage: compiler.webpack.Compilation.PROCESS_ASSETS_STAGE_ADDITIONAL,
        },
        async (_assets) => {
          for (const { name, data } of await this.#assets) {
            compilation.emitAsset(`pyodide/${name}`, new compiler.webpack.sources.RawSource(data));
          }
        },
      );
    });
  }
}
