import * as Comlink from "comlink";
import type * as monaco from "monaco-editor";
import { loadPyodide, PyodideInterface } from "pyodide";

// defined via webpack.DefinePlugin
declare let FOXGLOVE_SDK_WHEEL_FILENAME: string;

type CompletionItem = {
  type: string;
  name: string;
  prefix_len: number;
  doc: string;
};
type GetCompletionItems = (
  code: string,
  line: number,
  col: number,
) => { destroy(): void } & Array<CompletionItem>;

type GetSignaturesResultItem = {
  index: number | undefined;
  str: string;
  doc: string;
  params: Array<{ name: string }>;
};
type GetSignatures = (
  code: string,
  line: number,
  col: number,
) => { destroy(): void } & Array<GetSignaturesResultItem>;

export class RunnerWorker {
  #abortController = new AbortController();
  #pyodide: Promise<PyodideInterface>;
  #getCompletionItems: Promise<GetCompletionItems>;
  #getSignatures: Promise<GetSignatures>;
  #stdoutCallback: (output: string) => void = (output) => {
    console.log("[stdout]", output);
  };
  constructor() {
    this.#pyodide = this.#setup();
    this.#getCompletionItems = this.#pyodide.then(
      (pyodide) =>
        pyodide.runPython(
          `
            import jedi
            from pyodide.ffi import to_js
            def get_completion_items(code, line, col):
              completions = jedi.Script(code).complete(line, col - 1)
              return to_js([
                {
                  "type": completion.type,
                  "name": completion.name_with_symbols,
                  "prefix_len": completion.get_completion_prefix_length(),
                  "doc": completion.docstring(),
                }
                for completion in completions
                if completion.module_name == "__main__" or not completion.name.startswith("_")
              ])
            get_completion_items
          `,
          // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
          { globals: pyodide.toPy({}) },
        ) as GetCompletionItems,
    );
    this.#getSignatures = this.#pyodide.then(
      (pyodide) =>
        pyodide.runPython(
          `
            import jedi
            from pyodide.ffi import to_js
            def get_signatures(code, line, col):
              signatures = jedi.Script(code).get_signatures(line, col - 1)
              return to_js([
                {
                  "index": signature.index,
                  "str": signature.to_string(),
                  "doc": signature.docstring(),
                  "params": [
                    {
                      "name": param.name,
                    }
                    for param in signature.params
                  ],
                }
                for signature in signatures
                if signature.module_name == "__main__" or not signature.name.startswith("_")
              ])
            get_signatures
          `,
          // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
          { globals: pyodide.toPy({}) },
        ) as GetSignatures,
    );
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
    await pyodide.loadPackage("jedi");
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
    pyodide.runPython(
      `
        import os, pathlib, shutil
        os.chdir("/home/pyodide")
        try:
          shutil.rmtree("/home/pyodide/playground")
        except FileNotFoundError:
          pass
        pathlib.Path("/home/pyodide/playground").mkdir(parents=True)
        os.chdir("/home/pyodide/playground")
      `,
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
      { globals: pyodide.toPy({}) },
    );
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

  async getCompletionItems(
    code: string,
    line: number,
    col: number,
  ): Promise<monaco.languages.CompletionItem[]> {
    const getCompletionItems = await this.#getCompletionItems;
    return getCompletionItems(code, line, col).map((item) => normalizeCompletion(item, line, col));
  }

  async getSignatureHelp(
    code: string,
    line: number,
    col: number,
  ): Promise<monaco.languages.SignatureHelp> {
    const getSignatures = await this.#getSignatures;
    return {
      activeParameter: 0,
      activeSignature: 0,
      signatures: getSignatures(code, line, col).map(normalizeSignatureHelp),
    };
  }
}

function normalizeCompletion(
  item: CompletionItem,
  line: number,
  col: number,
): monaco.languages.CompletionItem {
  let kind = 25 satisfies monaco.languages.CompletionItemKind.User;
  switch (item.type) {
    case "module":
      kind = 8 satisfies monaco.languages.CompletionItemKind.Module;
      break;
    case "class":
      kind = 5 satisfies monaco.languages.CompletionItemKind.Class;
      break;
    case "instance":
      kind = 4 satisfies monaco.languages.CompletionItemKind.Variable;
      break;
    case "function":
      kind = 1 satisfies monaco.languages.CompletionItemKind.Function;
      break;
    case "param":
      kind = 4 satisfies monaco.languages.CompletionItemKind.Variable;
      break;
    case "path":
      kind = 20 satisfies monaco.languages.CompletionItemKind.File;
      break;
    case "keyword":
      kind = 17 satisfies monaco.languages.CompletionItemKind.Keyword;
      break;
    case "property":
      kind = 9 satisfies monaco.languages.CompletionItemKind.Property;
      break;
    case "statement":
      kind = 4 satisfies monaco.languages.CompletionItemKind.Variable;
      break;
  }
  return {
    insertText: item.name,
    label: item.name,
    kind,
    range: {
      startLineNumber: line,
      startColumn: col - item.prefix_len,
      endLineNumber: line,
      endColumn: col,
    },
    detail: item.doc,
  };
}

function normalizeSignatureHelp(
  item: GetSignaturesResultItem,
): monaco.languages.SignatureInformation {
  return {
    label: item.str,
    activeParameter: item.index,
    parameters: item.params.map((param) => ({ label: param.name })),
    documentation: item.doc,
  };
}

Comlink.expose(new RunnerWorker());
