import { ChildProcess, spawn } from "node:child_process";
import { SIGTERM } from "node:constants";
import { access, readdir } from "node:fs/promises";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";

import { testOnlyExports } from "./runPythonSdkExamples";

vi.mock("node:child_process", async (importOriginal) => ({
  ...(await importOriginal<typeof import("node:child_process")>()),
  spawn: vi.fn(),
}));

vi.mock("node:fs/promises", async (importOriginal) => ({
  ...(await importOriginal<typeof import("node:fs/promises")>()),
  readdir: vi.fn(),
}));

describe("main", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.mocked(spawn).mockReset();
    vi.mocked(readdir).mockReset();
  });

  it("skips the so101-visualization example because it requires a physical robot", async () => {
    const actualFs = await vi.importActual<typeof import("node:fs/promises")>("node:fs/promises");
    vi.mocked(readdir).mockImplementation(async (...args) => {
      const entries = await actualFs.readdir(...args);
      return entries.filter((entry) => entry.name.toString() === "so101-visualization");
    });
    await testOnlyExports.main({ timeout: "5000", installSdkFromPath: false });

    expect(spawn).not.toHaveBeenCalled();
  });
});

describe("getExampleArgs", () => {
  it.each(["ws-stream-mcap", "ws-playback-control-mcap"])(
    "supplies an existing MCAP input file for %s",
    async (example) => {
      const file = path.resolve(__dirname, "fixtures/empty.mcap");
      expect(await testOnlyExports.getExampleArgs(example)).toEqual(["--file", file]);
      await expect(access(file)).resolves.toBeUndefined();
    },
  );

  it("supplies an existing video file for rgb-camera-visualization", async () => {
    const file = path.resolve(__dirname, "fixtures/camera.avi");
    expect(await testOnlyExports.getExampleArgs("rgb-camera-visualization")).toEqual([
      "--camera-id",
      file,
    ]);
    await expect(access(file)).resolves.toBeUndefined();
  });
});

describe("runExample", () => {
  const timeoutMillis = 100;
  let dependencies: ChildProcess;
  let sdk: ChildProcess;
  let example: ChildProcess;
  const killMocks = new Map<ChildProcess, MockInstance<ChildProcess["kill"]>>();

  beforeEach(() => {
    vi.useFakeTimers();
    dependencies = new ChildProcess();
    sdk = new ChildProcess();
    example = new ChildProcess();
    killMocks.clear();
    vi.mocked(spawn)
      .mockReturnValueOnce(dependencies)
      .mockReturnValueOnce(sdk)
      .mockReturnValueOnce(example);
    for (const child of [dependencies, sdk, example]) {
      killMocks.set(
        child,
        vi.spyOn(child, "kill").mockImplementation(() => {
          child.emit("exit", null, "SIGTERM");
          return true;
        }),
      );
    }
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
    vi.mocked(spawn).mockReset();
  });

  it("waits for dependencies and the local SDK before starting the example timeout", async () => {
    vi.stubEnv("UV_PROJECT_ENVIRONMENT", "/tmp/other-environment");
    vi.stubEnv("RUNNER_TEST_ENV", "inherited");
    const run = testOnlyExports.runExample("ws-playback-control-mcap", {
      timeoutMillis,
      installSdkFromPath: true,
    });

    await vi.advanceTimersByTimeAsync(timeoutMillis * 2);
    expect(killMocks.get(dependencies)).not.toHaveBeenCalled();
    expect(spawn).toHaveBeenCalledTimes(1);

    dependencies.emit("exit", 0, null);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(timeoutMillis * 2);
    expect(killMocks.get(sdk)).not.toHaveBeenCalled();
    expect(spawn).toHaveBeenCalledTimes(2);

    sdk.emit("exit", 0, null);
    await vi.advanceTimersByTimeAsync(0);

    const dir = path.resolve(__dirname, "../python/foxglove-sdk-examples/ws-playback-control-mcap");
    const python = path.join(
      dir,
      ".venv",
      process.platform === "win32" ? "Scripts/python.exe" : "bin/python",
    );
    expect(vi.mocked(spawn).mock.calls.map(([command, args]) => [command, args])).toEqual([
      ["uv", ["sync"]],
      ["uv", ["pip", "install", "--python", python, "../../foxglove-sdk"]],
      [python, ["main.py", "--file", path.resolve(__dirname, "fixtures/empty.mcap")]],
    ]);
    for (const [, , options] of vi.mocked(spawn).mock.calls) {
      expect(options.env != undefined).toBe(true);
      expect(options.env?.UV_PROJECT_ENVIRONMENT).toBeUndefined();
      expect(options.env?.RUNNER_TEST_ENV).toBe("inherited");
    }

    await vi.advanceTimersByTimeAsync(timeoutMillis - 1);
    expect(killMocks.get(example)).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    await expect(run).resolves.toBeUndefined();
    expect(killMocks.get(example)).toHaveBeenCalledWith(SIGTERM);
  });

  it("runs against the published SDK without a local install and clears the timer on exit", async () => {
    vi.mocked(spawn).mockReset().mockReturnValueOnce(dependencies).mockReturnValueOnce(example);
    const run = testOnlyExports.runExample("quickstart", {
      timeoutMillis,
      installSdkFromPath: false,
    });
    dependencies.emit("exit", 0, null);
    await vi.advanceTimersByTimeAsync(0);
    expect(spawn).toHaveBeenCalledTimes(2);
    expect(vi.mocked(spawn).mock.calls[1]?.[1]).toEqual(["main.py"]);

    example.emit("exit", 0, null);
    await expect(run).resolves.toBeUndefined();
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(timeoutMillis);
    expect(killMocks.get(example)).not.toHaveBeenCalled();
  });

  it.each<[string, "dependencies" | "sdk" | "example", number | null, NodeJS.Signals | null]>([
    ["failed dependency installation", "dependencies", 1, null],
    ["interrupted local SDK installation", "sdk", null, "SIGTERM"],
    ["an example startup error", "example", 2, null],
    ["unexpected example termination", "example", null, "SIGTERM"],
  ])("rejects %s", async (_description, stage, code, signal) => {
    const run = testOnlyExports.runExample("quickstart", {
      timeoutMillis,
      installSdkFromPath: true,
    });
    const rejected = expect(run).rejects.toThrow(
      code == null ? (signal ?? "unknown") : `code ${code}`,
    );
    if (stage !== "dependencies") {
      dependencies.emit("exit", 0, null);
      await vi.advanceTimersByTimeAsync(0);
    }
    if (stage === "example") {
      sdk.emit("exit", 0, null);
      await vi.advanceTimersByTimeAsync(0);
    }
    const children = { dependencies, sdk, example };
    children[stage].emit("exit", code, signal);
    await rejected;
    expect(spawn).toHaveBeenCalledTimes({ dependencies: 1, sdk: 2, example: 3 }[stage]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("propagates process launch errors", async () => {
    const run = testOnlyExports.runExample("quickstart", {
      timeoutMillis,
      installSdkFromPath: true,
    });
    const rejected = expect(run).rejects.toThrow("uv not found");
    dependencies.emit("error", new Error("uv not found"));
    await rejected;
  });
});
