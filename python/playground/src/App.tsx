import { useCallback, useEffect, useRef, useState } from "react";
import "./App.css";
import { loadPyodide, version as pyodideVersion, type PyodideInterface } from "pyodide";
import { EventEmitter } from "eventemitter3";
import { McapIndexedReader } from "@mcap/core";
import { loadDecompressHandlers } from "@mcap/support";

type EventMap = {
  ready: () => void;
};

class Runner extends EventEmitter<EventMap> {
  #pyodide: Promise<PyodideInterface>;
  #abortController = new AbortController();
  #output: HTMLElement;

  constructor({ output }: { output: HTMLElement }) {
    super();
    this.#output = output;
    this.#pyodide = this.#setup();
  }

  async #setup(): Promise<PyodideInterface> {
    const wheelUrl = new URL(
      "/foxglove_sdk-0.11.0-cp312-cp312-emscripten_3_1_58_wasm32.whl",
      window.location.href,
    );
    let pyodide = await loadPyodide({
      indexURL: `https://cdn.jsdelivr.net/pyodide/v${pyodideVersion}/full/`,
    });
    pyodide.setStdout({
      batched: (output) => {
        this.#abortController.signal.throwIfAborted();
        this.#output.appendChild(document.createTextNode(output + "\n"));
      },
    });
    this.#abortController.signal.throwIfAborted();
    await pyodide.loadPackage("micropip");
    this.#abortController.signal.throwIfAborted();
    const micropip = pyodide.pyimport("micropip");
    await micropip.install(wheelUrl.href);
    this.#abortController.signal.throwIfAborted();
    this.emit("ready");
    return pyodide;
  }

  async run(code: string) {
    const pyodide = await this.#pyodide;
    try {
      pyodide.FS.unlink("/home/pyodide/quickstart-python.mcap");
    } catch (_err) {}
    pyodide.runPython(code);
  }

  async readFile() {
    return (await this.#pyodide).FS.readFile("/home/pyodide/quickstart-python.mcap");
  }

  dispose(): void {
    // avoid "uncaught" exception from the abort
    this.#pyodide.catch(() => {});
    this.#abortController.abort();
  }
}

export function App() {
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const outputRef = useRef<HTMLPreElement>(null);
  const runnerRef = useRef<Runner>(undefined);

  useEffect(() => {
    setReady(false);
    const runner = new Runner({
      output: outputRef.current!,
    });
    runner.on("ready", () => {
      setReady(true);
    });
    runnerRef.current = runner;
    return () => {
      runner.dispose();
      runnerRef.current = undefined;
    };
  }, []);

  const run = useCallback(async () => {
    const runner = runnerRef.current;
    if (!runner) {
      return;
    }
    await runner.run(inputRef.current?.value ?? "");

    let file = await runner.readFile();
    const reader = await McapIndexedReader.Initialize({
      readable: {
        async size() {
          return BigInt(file.length);
        },
        async read(offset, size) {
          return file.slice(Number(offset), Number(offset + size));
        },
      },
      decompressHandlers: await loadDecompressHandlers(),
    });
    console.log(reader);
  }, []);

  const [ready, setReady] = useState(false);

  return (
    <>
      <button onClick={run} disabled={!ready}>
        Run
      </button>
      <div style={{ display: "flex", gap: 16 }}>
        <textarea
          ref={inputRef}
          defaultValue={DEFAULT_CODE}
          style={{ flex: "1 1 0", minWidth: 0, minHeight: 0 }}
        />
        <pre
          ref={outputRef}
          style={{ flex: "1 1 0", minWidth: 0, minHeight: 0, border: "1px solid gray" }}
        ></pre>
      </div>
    </>
  );
}

const DEFAULT_CODE = `\
import foxglove
from foxglove import Channel
from foxglove.channels import SceneUpdateChannel
from foxglove.schemas import (
  Color,
  CubePrimitive,
  SceneEntity,
  SceneUpdate,
  Vector3,
)

foxglove.set_log_level("DEBUG")

file_name = "quickstart-python.mcap"
with foxglove.open_mcap(file_name) as writer:
  scene_channel = SceneUpdateChannel("/scene")
  for i in range(10):
    size = 1 + 0.2 * i
    scene_channel.log(
        SceneUpdate(
            entities=[
                SceneEntity(
                    cubes=[
                        CubePrimitive(
                            size=Vector3(x=size, y=size, z=size),
                            color=Color(r=1.0, g=0, b=0, a=1.0),
                        )
                    ],
                ),
            ]
        ),
        log_time=i * 200_000_000,
    )
`;
