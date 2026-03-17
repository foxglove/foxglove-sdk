// Adapted from @pyodide/webpack-plugin 1.4.0 (MPL-2.0).
import CopyWebpackPlugin from "copy-webpack-plugin";
import assert from "node:assert";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import webpack from "webpack";

const thisDirname = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

type CopyPluginOptions = ConstructorParameters<typeof CopyWebpackPlugin>[0];

type PyodidePluginOptions = Omit<CopyPluginOptions, "patterns"> & {
  packageIndexUrl?: string;
  globalLoadPyodide?: boolean;
  outDirectory?: string;
  version?: string;
  pyodideDependencyPath?: string;
};

type PyodidePackageMetadata = {
  version: string;
  files?: string[];
};

type Pattern = {
  from: string;
  to: string;
  transform?: {
    transformer: (input: Buffer) => string;
  };
};

const MIN_SUPPORTED_PYODIDE_VERSION = "0.24.0";

function choosePyodideFiles(pkg: PyodidePackageMetadata): string[] {
  const files = pkg.files ?? [];
  const ignore = [/^pyodide.m?js.*/, /.+\.d\.ts$/, /.+\.html$/];
  const filtered = files.filter((file) => !ignore.some((pattern) => pattern.test(file)));
  if (!filtered.includes("package.json")) {
    filtered.push("package.json");
  }
  return filtered;
}

function chooseAndTransformPatterns(
  pkg: PyodidePackageMetadata,
  packageIndexUrl?: string,
): Pattern[] {
  const resolvedIndexUrl =
    packageIndexUrl ?? `https://cdn.jsdelivr.net/pyodide/v${pkg.version}/full/`;
  return choosePyodideFiles(pkg).map((name) => {
    let transform: Pattern["transform"];
    if (packageIndexUrl && name === "pyodide.asm.js") {
      transform = {
        transformer: (input) =>
          input
            .toString()
            .replace(
              "resolvePath(file_name,API.config.indexURL)",
              `resolvePath(file_name,"${resolvedIndexUrl}")`,
            ),
      };
    }
    return { from: name, to: name, transform };
  });
}

function resolvePyodidePackagePath(pyodideDependencyPath?: string): string {
  if (pyodideDependencyPath) {
    return path.resolve(pyodideDependencyPath);
  }

  return findPyodidePackageRoot(require.resolve("pyodide"));
}

function findPyodidePackageRoot(entrypoint: string): string {
  let currentPath = entrypoint;

  for (;;) {
    const stat = fs.statSync(currentPath);
    if (stat.isFile()) {
      currentPath = path.dirname(currentPath);
      continue;
    }

    if (!stat.isDirectory()) {
      break;
    }

    const packageJsonPath = path.join(currentPath, "package.json");
    if (fs.existsSync(packageJsonPath)) {
      try {
        const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf-8")) as {
          name?: string;
        };
        if (packageJson.name === "pyodide") {
          return currentPath;
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(
          `Unable to locate and parse pyodide package.json from ${packageJsonPath}. ${message}`,
        );
      }
    }

    if (path.basename(currentPath) === "node_modules") {
      break;
    }

    const parentPath = path.dirname(currentPath);
    if (parentPath === currentPath) {
      break;
    }
    currentPath = parentPath;
  }

  throw new Error(
    "Unable to locate the pyodide package. Set pyodideDependencyPath to override the lookup.",
  );
}

function resolvePyodidePackageMetadata(
  pyodidePackagePath: string,
  version?: string,
): PyodidePackageMetadata {
  const packageJsonPath = path.join(pyodidePackagePath, "package.json");
  try {
    const packageJson = JSON.parse(
      fs.readFileSync(packageJsonPath, "utf-8"),
    ) as PyodidePackageMetadata;
    return version == undefined ? packageJson : { ...packageJson, version };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Unable to read ${packageJsonPath}. ${message}`);
  }
}

export class PyodidePlugin extends CopyWebpackPlugin {
  public readonly globalLoadPyodide: boolean;

  public constructor(options: PyodidePluginOptions = {}) {
    const {
      globalLoadPyodide = false,
      outDirectory: requestedOutDirectory = "pyodide",
      packageIndexUrl,
      pyodideDependencyPath,
      version,
      ...copyPluginOptions
    } = options;

    const outDirectory = requestedOutDirectory.startsWith("/")
      ? requestedOutDirectory.slice(1)
      : requestedOutDirectory;
    const pyodidePackagePath = resolvePyodidePackagePath(pyodideDependencyPath);
    const pkg = resolvePyodidePackageMetadata(pyodidePackagePath, version);
    const patterns = chooseAndTransformPatterns(pkg, packageIndexUrl).map((pattern) => ({
      from: path.resolve(pyodidePackagePath, pattern.from),
      to: path.join(outDirectory, pattern.to),
      transform: pattern.transform,
    }));

    assert.ok(
      patterns.length > 0,
      `Unsupported version of pyodide. Must use >=${MIN_SUPPORTED_PYODIDE_VERSION}`,
    );

    super({ ...copyPluginOptions, patterns });
    this.globalLoadPyodide = globalLoadPyodide;
  }

  public override apply(compiler: webpack.Compiler): void {
    super.apply(compiler);
    compiler.hooks.compilation.tap(this.constructor.name, (compilation) => {
      const compilationHooks = webpack.NormalModule.getCompilationHooks(compilation);
      compilationHooks.beforeLoaders.tap(this.constructor.name, (loaders, normalModule) => {
        const matches = /pyodide\.m?js$/.exec(normalModule.userRequest);
        if (!matches) {
          return;
        }

        loaders.push({
          loader: path.resolve(thisDirname, "pyodideLoader.cjs"),
          options: {
            globalLoadPyodide: this.globalLoadPyodide,
            isModule: matches[0].endsWith(".mjs"),
          },
          ident: "pyodide",
          type: null,
        });
      });
    });
  }
}
