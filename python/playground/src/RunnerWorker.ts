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

  async run(code: string): Promise<boolean> {
    const pyodide = await this.#pyodide;
    try {
      pyodide.FS.unlink("/home/pyodide/quickstart-python.mcap");
    } catch (err: unknown) {
      // ignore
    }
    pyodide.runPython(code);
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    const stat = pyodide.FS.stat("/home/pyodide/quickstart-python.mcap");
    // eslint-disable-next-line @typescript-eslint/no-unsafe-member-access
    return stat.size > 0;
  }

  async readFile(): Promise<Uint8Array<ArrayBuffer>> {
    const data = (await this.#pyodide).FS.readFile("/home/pyodide/quickstart-python.mcap");
    return Comlink.transfer(data as Uint8Array<ArrayBuffer>, [data.buffer]);
  }
}

Comlink.expose(new RunnerWorker());
