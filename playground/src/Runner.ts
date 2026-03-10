import * as Comlink from "comlink";
import EventEmitter from "eventemitter3";
import type * as monaco from "monaco-editor";

import type { RunnerWorker } from "./RunnerWorker";

type EventMap = {
  ready: () => void;
  ["run-completed"]: (mcapFilename: string | undefined) => void;
  ["set-layout"]: (layoutJson: string) => void;
};

export class Runner extends EventEmitter<EventMap> {
  #worker: Worker;
  #remote: Comlink.Remote<RunnerWorker>;

  constructor() {
    super();
    this.#worker = new Worker(new URL("./RunnerWorker", import.meta.url));
    this.#remote = Comlink.wrap(this.#worker);
    void this.#remote.onReady(
      Comlink.proxy(() => {
        this.emit("ready");
      }),
    );
    void this.#remote.onSetLayout(
      Comlink.proxy((layoutJson) => {
        this.emit("set-layout", layoutJson);
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

  async getHover(
    code: string,
    line: number,
    col: number,
  ): Promise<monaco.languages.Hover | undefined> {
    return await this.#remote.getHover(code, line, col);
  }

  async getReferenceRanges(code: string, line: number, col: number): Promise<monaco.IRange[]> {
    return await this.#remote.getReferenceRanges(code, line, col);
  }

  dispose(): void {
    this.#remote[Comlink.releaseProxy]();
    this.#worker.terminate();
  }
}
