import * as monaco from "monaco-editor";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

import { Runner } from "./Runner";

type EditorProps = {
  initialValue?: string;
  // eslint-disable-next-line react/no-unused-prop-types
  onSave?: () => void;
  runner: React.RefObject<Runner | undefined>;
};

export type EditorInterface = {
  getValue: () => string;
};

export const Editor = forwardRef<EditorInterface, EditorProps>(
  function Editor(props, ref): React.JSX.Element {
    const { initialValue, runner } = props;
    const latestProps = useRef(props);
    useEffect(() => {
      latestProps.current = props;
    }, [props]);
    const containerRef = useRef<HTMLDivElement>(null);
    const editorRef = useRef<monaco.editor.IStandaloneCodeEditor>(null);
    useEffect(() => {
      if (!containerRef.current) {
        return;
      }
      const editor = monaco.editor.create(containerRef.current, {
        value: initialValue,
        language: "python",
        automaticLayout: true,
        tabSize: 2,
        minimap: { enabled: false },
      });

      // Provide autocompletion/intellisense using jedi, based on https://github.com/pybricks/pybricks-code
      const completionProvider = monaco.languages.registerCompletionItemProvider("python", {
        triggerCharacters: ["."],
        async provideCompletionItems(model, position, _context, _token) {
          return {
            suggestions:
              (await runner.current?.getCompletionItems(
                model.getValue(),
                position.lineNumber,
                position.column,
              )) ?? [],
          };
        },
      });
      const signatureProvider = monaco.languages.registerSignatureHelpProvider("python", {
        signatureHelpTriggerCharacters: ["(", ","],
        signatureHelpRetriggerCharacters: [")"],
        async provideSignatureHelp(model, position, _token, _context) {
          return {
            dispose() {
              // noop
            },
            value: (await runner.current?.getSignatureHelp(
              model.getValue(),
              position.lineNumber,
              position.column,
            )) ?? {
              signatures: [],
              activeSignature: 0,
              activeParameter: 0,
            },
          };
        },
      });
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        latestProps.current.onSave?.();
      });
      editorRef.current = editor;
      return () => {
        signatureProvider.dispose();
        completionProvider.dispose();
        editor.dispose();
        editorRef.current = null;
      };
    }, [initialValue, runner]);

    useImperativeHandle(
      ref,
      () => ({
        getValue() {
          return editorRef.current?.getValue() ?? "";
        },
      }),
      [],
    );

    return <div className="editor" ref={containerRef}></div>;
  },
);
