import * as monaco from "monaco-editor";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

type EditorProps = {
  initialValue?: string;
  // eslint-disable-next-line react/no-unused-prop-types
  onSave?: () => void;
};

export type EditorInterface = {
  getValue: () => string;
};

export const Editor = forwardRef<EditorInterface, EditorProps>(
  function Editor(props, ref): React.JSX.Element {
    const { initialValue } = props;
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
      });
      editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
        latestProps.current.onSave?.();
      });
      editorRef.current = editor;
      return () => {
        editor.dispose();
        editorRef.current = null;
      };
    }, [initialValue]);

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
