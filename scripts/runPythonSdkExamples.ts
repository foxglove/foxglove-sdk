import { program } from "commander";
import { spawn } from "node:child_process";
import { SIGTERM } from "node:constants";
import { mkdtemp, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

/**
 * Run each example in the Python SDK, after installing dependencies.
 *
 * Many of the examples start a live server which is run until interrupted; all examples are run
 * with a timeout (default 5s). These are run serially since they use the default Foxglove port
 * number and, for simplicity, don't illustrate that configuration.
 */

const pyExamplesDir = path.resolve(__dirname, "../python/foxglove-sdk-examples");

async function main({ timeout }: { timeout: string }) {
  for (const example of await readdir(pyExamplesDir)) {
    await installDependencies(example);
    await runExample(example, parseInt(timeout));
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

async function installDependencies(name: string) {
  const dir = path.join(pyExamplesDir, name);
  return await new Promise((resolve, reject) => {
    const child = spawn("poetry", ["install"], {
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

async function newTempFile() {
  const prefix = `${tmpdir()}${path.sep}`;
  const dir = await mkdtemp(prefix);
  return path.join(dir, "test.mcap");
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
  .action(main)
  .parse();
