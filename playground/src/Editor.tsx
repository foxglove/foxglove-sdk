import { useTheme } from "@mui/material";
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
  setValue: (value: string) => void;
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
      // Provides hover tooltips
      const hoverProvider = monaco.languages.registerHoverProvider("python", {
        async provideHover(model, position, _token, _context) {
          return (
            (await runner.current?.getHover(
              model.getValue(),
              position.lineNumber,
              position.column,
            )) ?? {
              contents: [],
            }
          );
        },
      });
      // Provides highlight ranges to show other occurrences of a symbol in the document
      const highlightProvider = monaco.languages.registerDocumentHighlightProvider("python", {
        async provideDocumentHighlights(model, position, _token) {
          return (
            await runner.current?.getReferenceRanges(
              model.getValue(),
              position.lineNumber,
              position.column,
            )
          )?.map((range) => ({ range }));
        },
      });
      // Performs renaming of a symbol
      const renameProvider = monaco.languages.registerRenameProvider("python", {
        async resolveRenameLocation(model, position, _token) {
          // Check that jedi is able to find a symbol to rename
          const ranges =
            (await runner.current?.getReferenceRanges(
              model.getValue(),
              position.lineNumber,
              position.column,
            )) ?? [];
          if (ranges.length > 0) {
            // allow rename to proceed
            return undefined;
          }
          return {
            rejectReason: "You cannot rename this element.",
            range: { startLineNumber: 0, startColumn: 0, endLineNumber: 0, endColumn: 0 },
            text: "",
          };
        },
        async provideRenameEdits(
          model,
          position,
          newName,
          _token,
        ): Promise<monaco.languages.WorkspaceEdit & monaco.languages.Rejection> {
          const ranges = await runner.current?.getReferenceRanges(
            model.getValue(),
            position.lineNumber,
            position.column,
          );
          return {
            rejectReason: ranges ? undefined : "Rename operation failed.",
            edits:
              ranges?.map((item) => ({
                resource: model.uri,
                versionId: model.getVersionId(),
                textEdit: {
                  range: item,
                  text: newName,
                },
              })) ?? [],
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
        hoverProvider.dispose();
        highlightProvider.dispose();
        renameProvider.dispose();
        editor.dispose();
        editorRef.current = null;
      };
    }, [initialValue, runner]);

    const isDark = useTheme().palette.mode === "dark";
    useEffect(() => {
      monaco.editor.setTheme(isDark ? "vs-dark" : "vs");
    }, [isDark]);

    useImperativeHandle(
      ref,
      () => ({
        getValue() {
          return editorRef.current?.getValue() ?? "";
        },
        setValue(value: string) {
          editorRef.current?.setValue(value);
        },
      }),
      [],
    );

    return <div className="editor" ref={containerRef}></div>;
  },
);
