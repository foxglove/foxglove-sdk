import { program } from "commander";
import { spawn } from "node:child_process";
import { SIGTERM, F_OK } from "node:constants";
import { mkdtemp, readdir, rm, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

/**
 * Run each example in the Python SDK, after installing dependencies.
 *
 * If `--install-sdk-from-path` is passed, the local SDK will be installed into each example's
 * environment. CI uses this to test with the latest SDK; by default, examples specify the
 * published version in their pyproject.toml.
 *
 * Many of the examples start a live server which is run until interrupted; all examples are run
 * with a timeout (default 5s), after installation finishes. These are run serially since they
 * use the default Foxglove port number and, for simplicity, don't illustrate that configuration.
 */

const pyExamplesDir = path.resolve(__dirname, "../python/foxglove-sdk-examples");

const tempFiles: string[] = [];

async function main(opts: { timeout: string; installSdkFromPath: boolean }): Promise<void> {
  const { timeout, installSdkFromPath } = opts;
  const timeoutMillis = parseInt(timeout);

  const entries = await readdir(pyExamplesDir, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isDirectory()) {
      continue;
    }

    // Ignore directories that do not contain a main.py script.
    const script = path.join(pyExamplesDir, entry.name, "main.py");
    try {
      await access(script, F_OK);
    } catch {
      continue;
    }

    // Skip examples that require external credentials, services, or hardware, or that import
    // `foxglove.remote_access`, which the local SDK build doesn't include.
    const skipList = [
      "remote-access",
      "asset-server",
      "oak-camera-streaming",
      "dataset-training",
      "so101-visualization",
    ];
    if (skipList.includes(entry.name)) {
      console.debug(`Skipping example ${entry.name}`);
      continue;
    }

    console.debug(`Install & run example ${entry.name}`);
    await runExample(entry.name, { timeoutMillis, installSdkFromPath });
  }
}

async function runExample(
  name: string,
  opts: { timeoutMillis: number; installSdkFromPath: boolean },
): Promise<void> {
  const dir = path.join(pyExamplesDir, name);
  const python = path.join(
    dir,
    ".venv",
    process.platform === "win32" ? "Scripts/python.exe" : "bin/python",
  );

  await runExampleCommand(name, "uv", ["sync"], { cwd: dir });
  if (opts.installSdkFromPath) {
    await runExampleCommand(
      name,
      "uv",
      ["pip", "install", "--python", python, "../../foxglove-sdk"],
      {
        cwd: dir,
      },
    );
  }

  const exampleArgs = await getExampleArgs(name);
  await runExampleCommand(name, python, ["main.py", ...exampleArgs], {
    cwd: dir,
    timeoutMillis: opts.timeoutMillis,
  });
}

async function runExampleCommand(
  name: string,
  command: string,
  args: string[],
  opts: { cwd: string; timeoutMillis?: number },
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: opts.cwd,
      stdio: "inherit",
      env: { ...process.env, UV_PROJECT_ENVIRONMENT: undefined },
    });
    let timedOut = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    child.once("error", (err) => {
      clearTimeout(timer);
      reject(err);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      if (code === 0 || (timedOut && (code === 143 || signal === "SIGTERM"))) {
        resolve(undefined);
      } else {
        const signalOrCode = code != undefined ? `code ${code}` : (signal ?? "unknown");
        reject(new Error(`Example ${name} command ${command} exited with ${signalOrCode}`));
      }
    });
    if (opts.timeoutMillis != undefined) {
      timer = setTimeout(() => {
        timedOut = true;
        child.kill(SIGTERM);
      }, opts.timeoutMillis);
    }
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

async function getExampleArgs(example: string): Promise<string[]> {
  switch (example) {
    case "ws-stream-mcap":
    case "ws-playback-control-mcap":
      return ["--file", path.resolve(__dirname, "fixtures/empty.mcap")];
    case "write-mcap-file":
      return ["--path", await newTempFile()];
    default:
      return [];
  }
}

export const testOnlyExports = { getExampleArgs, runExample, main };

if (require.main === module) {
  program
    .option("--timeout [duration]", "timeout for each example in milliseconds", "5000")
    .option("--install-sdk-from-path", "use local sdk instead of version from pyproject", false)
    .action(main)
    .hook("postAction", removeTempFiles)
    .parse();
}
