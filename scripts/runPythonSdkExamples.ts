import { program } from "commander";
import { spawn } from "node:child_process";
import { SIGTERM } from "node:constants";
import { mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

/**
 * Run each example in the Python SDK, after installing dependencies.
 *
 * If `--install-sdk-from-path` is passed, then the project dependencies will be updated to refer
 * to the SDK at a local path, relative to the example directory. CI uses this to test with the
 * latest SDK; by default, examples specify the published version in their pyproject.toml.
 *
 * Many of the examples start a live server which is run until interrupted; all examples are run
 * with a timeout (default 5s). These are run serially since they use the default Foxglove port
 * number and, for simplicity, don't illustrate that configuration.
 */

const pyExamplesDir = path.resolve(__dirname, "../python/foxglove-sdk-examples");

const tempFiles: string[] = [];

async function main(opts: { timeout: string; installSdkFromPath: boolean }) {
  for (const example of await readdir(pyExamplesDir)) {
    console.debug(`Install & run example ${example}`);
    await installDependencies(example, { installSdkFromPath: opts.installSdkFromPath });
    await runExample(example, parseInt(opts.timeout));
  }
}

async function runExample(name: string, timeoutMillis = 5000) {
  const dir = path.join(pyExamplesDir, name);
  const args = await extraArgs(name);
  return await new Promise((resolve, reject) => {
    const child = spawn("poetry", ["run", "python", "main.py", ...args], {
      cwd: dir,
    });
    child.stderr.on("data", (data: Buffer | string) => {
      console.debug(data.toString());
    });
    child.on("exit", (code, signal) => {
      if (code === 0 || signal === "SIGTERM") {
        resolve(undefined);
      } else {
        const signalOrCode = code != undefined ? `code ${code}` : (signal ?? "unknown");
        reject(new Error(`Example ${name} exited with ${signalOrCode}`));
      }
    });
    setTimeout(() => {
      child.kill(SIGTERM);
    }, timeoutMillis);
  });
}

async function installDependencies(name: string, opts: { installSdkFromPath: boolean }) {
  const dir = path.join(pyExamplesDir, name);
  return await new Promise((resolve, reject) => {
    const args = opts.installSdkFromPath ? ["add", "foxglove-sdk@../../foxglove-sdk"] : ["install"];
    const child = spawn("poetry", args, {
      cwd: dir,
    });
    child.stdout.on("data", (data: Buffer | string) => {
      console.debug(data.toString());
    });
    child.stderr.on("data", (data: Buffer | string) => {
      console.error(data.toString());
    });
    child.on("close", (code) => {
      if (code === 0) {
        resolve(undefined);
      } else {
        reject(new Error(`Failed to install dependencies for ${name}`));
      }
    });
  });
}

async function newTempFile(name = "test.mcap") {
  const prefix = `${tmpdir()}${path.sep}`;
  const dir = await mkdtemp(prefix);
  const file = path.join(dir, name);
  tempFiles.push(file);
  return file;
}

async function removeTempFiles() {
  for (const file of tempFiles) {
    try {
      await rm(file);
    } catch (err) {
      if (err instanceof Error && "code" in err && err.code === "ENOENT") {
        continue;
      }
      throw err;
    }
  }
}

async function extraArgs(example: string) {
  switch (example) {
    case "ws-stream-mcap":
      return ["--file", path.resolve(__dirname, "fixtures/empty.mcap")];
    case "write-mcap-file":
      return ["--path", await newTempFile()];
    default:
      return [];
  }
}

program
  .option("--timeout [duration]", "timeout for each example in milliseconds", "5000")
  .option("--install-sdk-from-path", "use local sdk instead of version from pyproject", false)
  .action(main)
  .hook("postAction", removeTempFiles)
  .parse();
