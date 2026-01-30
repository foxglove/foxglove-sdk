import ReactRefreshPlugin from "@pmmmwh/react-refresh-webpack-plugin";
import { PyodidePlugin } from "@pyodide/webpack-plugin";
import CopyWebpackPlugin from "copy-webpack-plugin";
import HtmlWebpackPlugin from "html-webpack-plugin";
import MonacoWebpackPlugin from "monaco-editor-webpack-plugin";
import fs from "node:fs";
import path from "node:path";
import { version as pyodideVersion } from "pyodide";
import reactRefreshTypescript from "react-refresh-typescript";
import webpack, { type Compiler, type Configuration } from "webpack";

const thisDirname = import.meta.dirname;

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
    output: {
      filename: "index.js",
      path: path.resolve(thisDirname, "dist"),
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
      ],
    },
    resolve: {
      extensions: [".tsx", ".ts", ".js"],
      fallback: {
        fs: false,
        path: false,
      },
    },
    plugins: [
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
      new PyodideCdnDownloadPlugin([
        // Pyodide is distributed with a list of packages that it knows about. These filenames match
        // the ones it will try to download at runtime when calling pyodide.loadPackage(). See the
        // list at: https://pyodide.org/en/stable/usage/packages-in-pyodide.html
        "jedi-0.19.1-py2.py3-none-any.whl",
        "micropip-0.9.0-py3-none-any.whl",
        "numpy-2.0.2-cp312-cp312-pyodide_2024_0_wasm32.whl",
        "openblas-0.3.26.zip",
        "opencv_python-4.10.0.84-cp312-cp312-pyodide_2024_0_wasm32.whl",
        "packaging-24.2-py3-none-any.whl",
        "pandas-2.2.3-cp312-cp312-pyodide_2024_0_wasm32.whl",
        "parso-0.8.4-py2.py3-none-any.whl",
        "protobuf-5.29.2-cp312-cp312-pyodide_2024_0_wasm32.whl",
        "python_dateutil-2.9.0.post0-py2.py3-none-any.whl",
        "pytz-2024.1-py2.py3-none-any.whl",
        "scipy-1.14.1-cp312-cp312-pyodide_2024_0_wasm32.whl",
        "six-1.16.0-py2.py3-none-any.whl",
      ]),
    ],
  };
};

/**
 * Download python wheel files from Pyodide's CDN at build time
 *
 * See available packages at: https://pyodide.org/en/stable/usage/packages-in-pyodide.html
 */
class PyodideCdnDownloadPlugin {
  #packages: string[];
  #assets: Promise<Array<{ name: string; data: Buffer }>>;

  constructor(packages: string[]) {
    this.#packages = packages;
    this.#assets = Promise.all(
      this.#packages.map(async (name) => {
        console.log("fetching", name);
        const url = `https://cdn.jsdelivr.net/pyodide/v${pyodideVersion}/full/${name}`;
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
