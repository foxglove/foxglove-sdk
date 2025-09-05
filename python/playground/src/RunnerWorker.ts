import * as Comlink from "comlink";
import { loadPyodide, PyodideInterface } from "pyodide";

// defined via webpack.DefinePlugin
declare let FOXGLOVE_SDK_WHEEL_FILENAME: string;

export class RunnerWorker {
  #abortController = new AbortController();
  #pyodide: Promise<PyodideInterface>;
  #stdoutCallback: (output: string) => void = (output) => {
    console.log("[stdout]", output);
  };
  constructor() {
    this.#pyodide = this.#setup();
  }

  onReady(callback: () => void): void {
    void this.#pyodide.then(() => {
      callback();
    });
  }

  onStdout(callback: (output: string) => void): void {
    this.#stdoutCallback = callback;
  }

  async #setup(): Promise<PyodideInterface> {
    const pyodide = await loadPyodide({
      indexURL: "/pyodide", // use files bundled by @pyodide/webpack-plugin
    });
    const wheelPath = `/home/pyodide/${FOXGLOVE_SDK_WHEEL_FILENAME}`;
    pyodide.FS.writeFile(
      wheelPath,
      new Uint8Array(await (await fetch(`/${FOXGLOVE_SDK_WHEEL_FILENAME}`)).arrayBuffer()),
    );
    pyodide.setStdout({
      batched: (output) => {
        this.#abortController.signal.throwIfAborted();
        this.#stdoutCallback(output);
      },
    });
    this.#abortController.signal.throwIfAborted();
    await pyodide.loadPackage("micropip");
    this.#abortController.signal.throwIfAborted();
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const micropip = pyodide.pyimport("micropip");
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call, @typescript-eslint/no-unsafe-member-access
    await micropip.install(`emfs:${wheelPath}`);
    this.#abortController.signal.throwIfAborted();
    return pyodide;
  }

  async run(code: string): Promise<string | undefined> {
    const pyodide = await this.#pyodide;
    try {
      pyodide.runPython(
        `
          import os, pathlib, shutil
          shutil.rmtree("/home/pyodide/playground", ignore_errors=True)
          pathlib.Path("/home/pyodide/playground").mkdir(parents=True)
          os.chdir("/home/pyodide/playground")
        `,
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
        { globals: pyodide.toPy({}) },
      );
    } catch (err: unknown) {
      // ignore
    }
    pyodide.runPython(code);
    return this.#getFileNames(pyodide)[0];
  }

  #getFileNames(pyodide: PyodideInterface): string[] {
    return (
      // eslint-disable-next-line @typescript-eslint/no-unsafe-call
      pyodide
        .runPython(
          `
            from glob import glob
            glob("*.mcap", root_dir="/home/pyodide/playground")
          `,
          // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
          { globals: pyodide.toPy({}) },
        )
        // eslint-disable-next-line @typescript-eslint/no-unsafe-member-access
        .toJs() as string[]
    );
  }

  async readFile(): Promise<{ name: string; data: Uint8Array<ArrayBuffer> }> {
    const pyodide = await this.#pyodide;
    const filename = this.#getFileNames(pyodide)[0];
    if (!filename) {
      throw new Error("No .mcap file found");
    }
    const data = pyodide.FS.readFile(`/home/pyodide/playground/${filename}`);
    return Comlink.transfer({ name: filename, data: data as Uint8Array<ArrayBuffer> }, [
      data.buffer,
    ]);
  }
}

Comlink.expose(new RunnerWorker());
