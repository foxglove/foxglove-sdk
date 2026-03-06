import { PlayFilledAlt, DocumentDownload } from "@carbon/icons-react";
import { DataSource, Layout, SelectLayoutParams } from "@foxglove/embed";
import { FoxgloveViewer, FoxgloveViewerInterface } from "@foxglove/embed-react";
import {
  Button,
  GlobalStyles,
  IconButton,
  MenuItem,
  Select,
  Tooltip,
  Typography,
} from "@mui/material";
import { Allotment } from "allotment";
import { useCallback, useEffect, useRef, useState } from "react";
import toast, { Toaster } from "react-hot-toast";
import { tss } from "tss-react/mui";

import { Editor, EditorInterface } from "./Editor";
import { Runner } from "./Runner";
import { DEFAULT_EXAMPLE, EXAMPLES, Example, FALLBACK_LAYOUT } from "./examples";
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
    // Match the height of the app bar in the Foxglove app
    height: "44px",
    padding: "0 8px 0 16px",
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
  exampleSelect: {
    "@container topBar (width < 480px)": {
      display: "none",
    },
  },
  controls: {
    display: "flex",
    flexGrow: 1,
    justifyContent: "flex-end",
    alignItems: "center",
    gap: 8,
  },
  toast: {
    fontSize: theme.typography.body1.fontSize,
    backgroundColor: theme.palette.background.paper,
    color: theme.palette.text.primary,
    boxShadow: theme.shadows[4],
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
  const onViewerError = useCallback((msg: string) => {
    toast.error(msg);
  }, []);

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
          opaqueLayout: DEFAULT_EXAMPLE.layout,
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
    runner.on("set-layout", (layoutJson) => {
      try {
        setSelectedLayout({
          storageKey: LAYOUT_STORAGE_KEY,
          layout: JSON.parse(layoutJson) as Layout,
          force: true,
        });
      } catch (error) {
        toast.error(`Error setting layout: ${String(error)}`);
      }
    });
    runnerRef.current = runner;
    return () => {
      runner.dispose();
      runnerRef.current = undefined;
    };
  }, []);

  const onExampleSelected = useCallback((example: Example) => {
    editorRef.current?.setValue(example.code);
    setSelectedLayout({
      storageKey: LAYOUT_STORAGE_KEY,
      opaqueLayout: example.layout ?? FALLBACK_LAYOUT,
      force: true,
    });
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
          layout,
        });
      })
      .catch((err: unknown) => {
        toast.error(`Sharing failed: ${String(err)}`);
      });
  }, []);

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
            <Select
              className={classes.exampleSelect}
              size="small"
              displayEmpty
              value=""
              onChange={(e) => {
                const example = EXAMPLES[Number(e.target.value)];
                if (example) {
                  onExampleSelected(example);
                }
              }}
              renderValue={() => "Examples"}
              sx={{ minWidth: 120, height: 32 }}
            >
              {EXAMPLES.map((example, index) => (
                <MenuItem key={index} value={index}>
                  {example.label}
                </MenuItem>
              ))}
            </Select>
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
          initialValue={initialState?.code ?? DEFAULT_EXAMPLE.code}
          onSave={share}
          runner={runnerRef}
        />
      </Allotment.Pane>
      <Allotment.Pane minSize={200}>
        <FoxgloveViewer
          ref={viewerRef}
          style={{ width: "100%", height: "100%", overflow: "hidden" }}
          data={dataSource}
          layout={selectedLayout}
          onError={onViewerError}
        />
      </Allotment.Pane>
    </Allotment>
  );
}

const LAYOUT_STORAGE_KEY = "playground-layout";
