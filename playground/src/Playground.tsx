import { PlayFilledAlt, DocumentDownload } from "@carbon/icons-react";
import { DataSource, SelectLayoutParams } from "@foxglove/embed";
import { FoxgloveViewer, FoxgloveViewerInterface } from "@foxglove/embed-react";
import { Button, GlobalStyles, IconButton, Tooltip, Typography } from "@mui/material";
import { Allotment } from "allotment";
import { useCallback, useEffect, useRef, useState } from "react";
import toast, { Toaster } from "react-hot-toast";
import { tss } from "tss-react/mui";

import { Editor, EditorInterface } from "./Editor";
import { Runner } from "./Runner";
import { getUrlState, setUrlState, UrlState } from "./urlState";

import "./Playground.css";
import "allotment/dist/style.css";

const useStyles = tss.create(({ theme }) => ({
  leftPane: {
    display: "flex",
    flexDirection: "column",
  },
  topBar: {
    flex: "0 0 auto",
    display: "flex",
    padding: "8px 8px 8px 16px",
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    borderBottom: `1px solid ${theme.palette.divider}`,
    backgroundColor: theme.palette.background.paper,
    color: theme.palette.text.primary,
    container: "topBar / inline-size",
  },
  title: {
    "@container topBar (width < 480px)": {
      display: "none",
    },
  },
  controls: {
    display: "flex",
    flexGrow: 1,
    justifyContent: "flex-end",
    gap: 8,
  },
  toast: {
    fontSize: theme.typography.body1.fontSize,
  },
  toastMonospace: {
    maxWidth: "none",
    fontFamily: theme.typography.fontMonospace,
    overflow: "hidden",
    div: {
      whiteSpace: "pre-wrap",
    },
  },
}));

function setAndCopyUrlState(state: UrlState) {
  setUrlState(state);
  navigator.clipboard.writeText(window.location.href).then(
    () => toast.success("URL copied to clipboard"),
    () => toast.error("Failed to copy URL"),
  );
}

export function Playground(): React.JSX.Element {
  const runnerRef = useRef<Runner>(undefined);
  const editorRef = useRef<EditorInterface>(null);
  const viewerRef = useRef<FoxgloveViewerInterface>(null);
  const { cx, classes } = useStyles();

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
    const runner = new Runner();
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

      try {
        const { name, data } = await runner.readFile();
        setDataSource({ type: "file", file: new File([data], name) });
      } catch (err) {
        toast.error(`Run failed: ${String(err)}`);
      }
    } catch (err) {
      toast.error(String(err), { className: cx(classes.toast, classes.toastMonospace) });
    }
  }, [classes, cx]);

  const share = useCallback(() => {
    const editor = editorRef.current;
    if (!editor) {
      return;
    }
    const viewer = viewerRef.current;
    if (!viewer) {
      return;
    }
    viewer
      .getLayout()
      .then((layout) => {
        setAndCopyUrlState({
          code: editor.getValue(),
          layout: layout ?? selectedLayout.opaqueLayout,
        });
      })
      .catch((err: unknown) => {
        toast.error(`Sharing failed: ${String(err)}`);
      });
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

  const [isRunning, setIsRunning] = useState(false);
  const onClickRun = useCallback(() => {
    setIsRunning(true);
    void run().finally(() => {
      setIsRunning(false);
    });
  }, [run]);

  return (
    <Allotment>
      <Allotment.Pane minSize={400} className={classes.leftPane}>
        <Toaster position="top-right" toastOptions={{ className: classes.toast }} />
        <GlobalStyles
          styles={(theme) => ({
            ":root": {
              // https://allotment.mulberryhousesoftware.com/docs/styling
              "--separator-border": theme.palette.divider,
              "--focus-border": theme.palette.divider,
              "--sash-hover-transition-duration": "0s",
            },
          })}
        />
        <div className={classes.topBar}>
          <Typography className={classes.title} variant="body1">
            Foxglove SDK Playground
          </Typography>
          <div className={classes.controls}>
            {mcapFilename && (
              <Tooltip title={`Download ${mcapFilename}`}>
                <IconButton onClick={() => void download()}>
                  <DocumentDownload />
                </IconButton>
              </Tooltip>
            )}
            <Button onClick={chooseLayout}>Upload layout</Button>
            <input
              ref={layoutInputRef}
              type="file"
              accept=".json"
              style={{ display: "none" }}
              onChange={onLayoutSelected}
            />
            <Button
              variant="contained"
              loading={!ready || isRunning}
              loadingPosition="start"
              onClick={onClickRun}
              startIcon={<PlayFilledAlt />}
            >
              Run
            </Button>
            <Button variant="outlined" onClick={share}>
              Share
            </Button>
          </div>
        </div>
        <Editor
          ref={editorRef}
          initialValue={initialState?.code ?? DEFAULT_CODE}
          onSave={share}
          runner={runnerRef}
        />
      </Allotment.Pane>
      <Allotment.Pane minSize={200}>
        <FoxgloveViewer
          ref={viewerRef}
          style={{ width: "100%", height: "100%", overflow: "hidden" }}
          colorScheme="light"
          data={dataSource}
          layout={selectedLayout}
        />
      </Allotment.Pane>
    </Allotment>
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
