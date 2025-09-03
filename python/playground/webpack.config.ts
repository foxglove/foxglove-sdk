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
  <head></head>
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
        "annotated_types-0.6.0-py3-none-any.whl",
        "certifi-2024.12.14-py3-none-any.whl",
        "charset_normalizer-3.3.2-py3-none-any.whl",
        "click-8.1.7-py3-none-any.whl",
        "distlib-0.3.8-py2.py3-none-any.whl",
        "httpx-0.28.1-py3-none-any.whl",
        "idna-3.7-py3-none-any.whl",
        "micropip-0.9.0-py3-none-any.whl",
        "packaging-24.2-py3-none-any.whl",
        "packaging-24.2-py3-none-any.whl",
        "pydantic_core-2.27.2-cp312-cp312-pyodide_2024_0_wasm32.whl",
        "pydantic-2.10.5-py3-none-any.whl",
        "requests-2.31.0-py3-none-any.whl",
        "rich-13.7.1-py3-none-any.whl",
        "ruamel.yaml-0.18.6-py3-none-any.whl",
        "typing_extensions-4.11.0-py3-none-any.whl",
        "urllib3-2.2.3-py3-none-any.whl",
      ]),
    ],
  };
};

/** Download python wheel files from Pyodide's CDN at build time */
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
