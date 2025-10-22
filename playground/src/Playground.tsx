import { DataSource, SelectLayoutParams } from "@foxglove/embed";
import { FoxgloveViewer } from "@foxglove/embed-react";
import { useCallback, useEffect, useRef, useState } from "react";
import toast, { Toaster } from "react-hot-toast";

import { Editor, EditorInterface } from "./Editor";
import { Runner } from "./Runner";
import { getUrlState, setUrlState, UrlState } from "./urlState";

import "./Playground.css";

function setAndCopyUrlState(state: UrlState) {
  setUrlState(state);
  navigator.clipboard.writeText(window.location.href).then(
    () => toast.success("URL copied to clipboard"),
    () => toast.error("Failed to copy URL"),
  );
}

export function Playground(): React.JSX.Element {
  const outputRef = useRef<HTMLPreElement>(null);
  const runnerRef = useRef<Runner>(undefined);
  const editorRef = useRef<EditorInterface>(null);

  const [initialState] = useState(() => {
    try {
      return getUrlState();
    } catch (err) {
      toast.error(`Unable to restore from URL: ${String(err)}`);
      return undefined;
    }
  });
  const [selectedLayout, setSelectedLayout] = useState<SelectLayoutParams>(
    initialState?.layout != undefined
      ? {
          storageKey: LAYOUT_STORAGE_KEY,
          opaqueLayout: initialState.layout,
          force: true,
        }
      : {
          storageKey: LAYOUT_STORAGE_KEY,
          opaqueLayout: DEFAULT_LAYOUT_DATA,
          force: false,
        },
  );
  const [ready, setReady] = useState(false);
  const [mcapFilename, setMcapFilename] = useState<string | undefined>();
  const [dataSource, setDataSource] = useState<DataSource | undefined>();
  const layoutInputRef = useRef<HTMLInputElement>(null);

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
      toast.error(`Run failed: ${String(err)}`);
    }
  }, []);

  const share = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    setAndCopyUrlState({ code: editor.getValue(), layout: selectedLayout });
  }, [selectedLayout]);

  const chooseLayout = useCallback(() => {
    layoutInputRef.current?.click();
  }, []);

  const onLayoutSelected = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) {
      return;
    }
    file
      .text()
      .then(JSON.parse)
      .then(
        (layout) => {
          setSelectedLayout({
            storageKey: LAYOUT_STORAGE_KEY,
            opaqueLayout: layout,
            force: true,
          });
          setAndCopyUrlState({ code: editorRef.current?.getValue() ?? "", layout });
        },
        (err: unknown) => {
          toast.error(`Failed to load layout: ${String(err)}`);
        },
      );
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
      toast.error(`Download failed: ${String(err)}`);
    }
  }, []);

  return (
    <div style={{ width: "100%", height: "100%", display: "flex", flexDirection: "column" }}>
      <Toaster />
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
          <button onClick={share}>Share</button>
          <button onClick={chooseLayout}>Choose layout…</button>
          <input
            ref={layoutInputRef}
            type="file"
            accept=".json"
            style={{ display: "none" }}
            onChange={onLayoutSelected}
          />
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
          <Editor
            ref={editorRef}
            initialValue={initialState?.code ?? DEFAULT_CODE}
            onSave={share}
            runner={runnerRef}
          />
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
          style={{ flex: "1 1 0", overflow: "hidden" }}
          colorScheme="light"
          data={dataSource}
          layout={selectedLayout}
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

scene_channel = SceneUpdateChannel("/scene")

with foxglove.open_mcap("playground.mcap") as writer:
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

const LAYOUT_STORAGE_KEY = "playground-layout";
const DEFAULT_LAYOUT_DATA = {
  configById: {
    "3D!1ehnpb2": {
      cameraState: {
        distance: 20,
        perspective: true,
        phi: 60,
        target: [0, 0, 0],
        targetOffset: [0, 0, 0],
        targetOrientation: [0, 0, 0, 1],
        thetaOffset: 45,
        fovy: 45,
        near: 0.5,
        far: 5000,
      },
      followMode: "follow-pose",
      scene: {},
      transforms: {},
      topics: {
        "/scene": {
          visible: true,
        },
      },
      layers: {
        grid: {
          visible: true,
          drawBehind: false,
          frameLocked: true,
          label: "Grid",
          instanceId: "7cfdaa56-0cc3-4576-b763-5a8882575cd4",
          layerId: "foxglove.Grid",
          size: 10,
          divisions: 10,
          lineWidth: 1,
          color: "#248eff",
          position: [0, 0, 0],
          rotation: [0, 0, 0],
        },
      },
      publish: {
        type: "point",
        poseTopic: "/move_base_simple/goal",
        pointTopic: "/clicked_point",
        poseEstimateTopic: "/initialpose",
        poseEstimateXDeviation: 0.5,
        poseEstimateYDeviation: 0.5,
        poseEstimateThetaDeviation: 0.26179939,
      },
      imageMode: {},
    },
    "RawMessages!2zn7j4u": {
      diffEnabled: false,
      diffMethod: "custom",
      diffTopicPath: "",
      showFullMessageForDiff: false,
      topicPath: "/scene",
      fontSize: 12,
    },
    "Plot!30ea437": {
      paths: [
        {
          value: "/scene.entities[:].cubes[0].size.x",
          enabled: true,
          timestampMethod: "receiveTime",
          label: "Cube size",
        },
      ],
      showXAxisLabels: true,
      showYAxisLabels: true,
      showLegend: true,
      legendDisplay: "floating",
      showPlotValuesInLegend: false,
      isSynced: true,
      xAxisVal: "timestamp",
      sidebarDimension: 240,
    },
  },
  globalVariables: {},
  userNodes: {},
  playbackConfig: {
    speed: 1,
  },
  drawerConfig: {
    tracks: [],
  },
  layout: {
    first: "3D!1ehnpb2",
    second: {
      direction: "column",
      second: "Plot!30ea437",
      first: "RawMessages!2zn7j4u",
      splitPercentage: 67.0375521557719,
    },
    direction: "row",
    splitPercentage: 60.57971014492753,
  },
};
