import { DataSource } from "@foxglove/embed";
import { FoxgloveViewer } from "@foxglove/embed-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Editor, EditorInterface } from "./Editor";
import { Runner } from "./Runner";

import "./Playground.css";

export function Playground(): React.JSX.Element {
  const outputRef = useRef<HTMLPreElement>(null);
  const runnerRef = useRef<Runner>(undefined);
  const editorRef = useRef<EditorInterface>(null);

  const [ready, setReady] = useState(false);
  const [mcapFilename, setMcapFilename] = useState<string | undefined>();
  const [dataSource, setDataSource] = useState<DataSource | undefined>();

  useEffect(() => {
    setReady(false);
    const runner = new Runner({
      output: outputRef.current!,
    });
    runner.on("ready", () => {
      setReady(true);
    });
    runner.on("run-completed", (value) => {
      setMcapFilename(value);
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
    try {
      await runner.run(editorRef.current?.getValue() ?? "");

      const { name, data } = await runner.readFile();
      setDataSource({ type: "file", file: new File([data], name) });
    } catch (err) {
      console.error("Run failed:", err);
    }
  }, []);

  const download = useCallback(async () => {
    const runner = runnerRef.current;
    if (!runner) {
      return;
    }
    try {
      const { name, data } = await runner.readFile();

      const link = document.createElement("a");
      link.style.display = "none";
      document.body.appendChild(link);

      const url = URL.createObjectURL(new Blob([data], { type: "application/octet-stream" }));
      link.setAttribute("download", name);
      link.setAttribute("href", url);
      link.click();
      requestAnimationFrame(() => {
        link.remove();
        URL.revokeObjectURL(url);
      });
    } catch (err) {
      console.error("Run failed:", err);
    }
  }, []);

  return (
    <div style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column" }}>
      <div
        style={{
          flex: "0 0 auto",
          display: "flex",
          padding: "8px 8px 8px 16px",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          backgroundColor: "#eee",
        }}
      >
        <div>Foxglove SDK Playground</div>
        <div style={{ display: "flex", gap: 8 }}>
          <button onClick={() => void download()} disabled={!mcapFilename}>
            Download {mcapFilename}
          </button>
          <button onClick={() => void run()} disabled={!ready}>
            Run
          </button>
        </div>
      </div>
      <div style={{ display: "flex", gap: 16, flex: "1 1 0", minWidth: 0, minHeight: 0 }}>
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            flex: "1 1 0",
            width: 0,
          }}
        >
          <Editor initialValue={DEFAULT_CODE} ref={editorRef} runner={runnerRef} />
          <pre
            ref={outputRef}
            style={{
              flex: "0 1 100px",
              minWidth: 0,
              minHeight: 0,
              border: "1px solid gray",
              borderLeft: "none",
              borderBottom: "none",
              overflow: "auto",
              margin: 0,
            }}
          ></pre>
        </div>

        <FoxgloveViewer
          data={dataSource}
          style={{ flex: "1 1 0", overflow: "hidden" }}
          colorScheme="light"
        />
      </div>
    </div>
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
