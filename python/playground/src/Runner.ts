import * as Comlink from "comlink";
import EventEmitter from "eventemitter3";

import type { RunnerWorker } from "./RunnerWorker";

type EventMap = {
  ready: () => void;
  // eslint-disable-next-line @foxglove/no-boolean-parameters
  ["has-mcap"]: (value: boolean) => void;
};

export class Runner extends EventEmitter<EventMap> {
  #worker: Worker;
  #remote: Comlink.Remote<RunnerWorker>;
  #output: HTMLElement;

  constructor({ output }: { output: HTMLElement }) {
    super();
    this.#output = output;
    this.#worker = new Worker(new URL("./RunnerWorker", import.meta.url));
    this.#remote = Comlink.wrap(this.#worker);
    void this.#remote.onReady(
      Comlink.proxy(() => {
        this.emit("ready");
      }),
    );
    void this.#remote.onStdout(
      Comlink.proxy((str) => {
        this.#output.appendChild(document.createTextNode(str + "\n"));
      }),
    );
  }

  async run(code: string): Promise<void> {
    this.emit("has-mcap", await this.#remote.run(code));
  }

  async readFile(): Promise<Uint8Array<ArrayBuffer>> {
    return await this.#remote.readFile();
  }

  dispose(): void {
    this.#remote[Comlink.releaseProxy]();
    this.#worker.terminate();
  }
}
