import * as Comlink from "comlink";
import EventEmitter from "eventemitter3";
import type * as monaco from "monaco-editor";

import type { RunnerWorker } from "./RunnerWorker";

type EventMap = {
  ready: () => void;
  ["run-completed"]: (mcapFilename: string | undefined) => void;
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
    this.emit("run-completed", await this.#remote.run(code));
  }

  async readFile(): Promise<{ name: string; data: Uint8Array<ArrayBuffer> }> {
    return await this.#remote.readFile();
  }

  async getCompletionItems(
    code: string,
    line: number,
    col: number,
  ): Promise<monaco.languages.CompletionItem[]> {
    return await this.#remote.getCompletionItems(code, line, col);
  }

  async getSignatureHelp(
    code: string,
    line: number,
    col: number,
  ): Promise<monaco.languages.SignatureHelp> {
    return await this.#remote.getSignatureHelp(code, line, col);
  }

  dispose(): void {
    this.#remote[Comlink.releaseProxy]();
    this.#worker.terminate();
  }
}
