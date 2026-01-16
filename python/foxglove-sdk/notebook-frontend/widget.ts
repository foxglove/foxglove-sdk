import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer } from "@foxglove/embed";
import type { Layout, SelectLayoutParams } from "@foxglove/embed";

// Specifies attributes defined with traitlets in ../python/foxglove/notebook/widget.py
interface WidgetModel {
  width: number | "full";
  height: number;
  src?: string;
  _layout?: string;
}

const DEFAULT_NOTEBOOK_LAYOUT_STORAGE_KEY = "foxglove-notebook-default-layout";

type Message = {
  type: "update-data";
};

function createSelectLayoutParams(layoutJson: string | undefined): SelectLayoutParams {
  // Even if no layout is provided, we want to always provide our storageKey and force=true so that
  // the embed doesn't fall back to its default caching behavior.
  return {
    storageKey: DEFAULT_NOTEBOOK_LAYOUT_STORAGE_KEY,
    force: true,
    layout: layoutJson ? (JSON.parse(layoutJson) as Layout) : undefined,
  };
}

function render({ model, el }: RenderProps<WidgetModel>): void {
  const parent = document.createElement("div");

  const initialLayoutJson = model.get("_layout");

  const viewer = new FoxgloveViewer({
    parent,
    embeddedViewer: "Python",
    src: model.get("src"),
    orgSlug: undefined,
    initialLayoutParams: createSelectLayoutParams(initialLayoutJson),
  });

  viewer.addEventListener("ready", () => {
    model.send({
      type: "ready",
    });
  });

  model.on("msg:custom", (msg: Message, buffers: DataView<ArrayBuffer>[]) => {
    // Only one message is supported currently, however let's keep the if clause to be explicit
    // and avoid future pitfalls
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    if (msg.type === "update-data") {
      const files = buffers.map((buffer, i) => new File([buffer.buffer], `data-${i}.mcap`));
      viewer.setDataSource({
        type: "file",
        file: files,
      });
    }
  });

  parent.style.width = model.get("width") === "full" ? "100%" : `${model.get("width")}px`;
  parent.style.height = `${model.get("height")}px`;

  model.on("change:width", () => {
    parent.style.width = model.get("width") === "full" ? "100%" : `${model.get("width")}px`;
  });

  model.on("change:height", () => {
    parent.style.height = `${model.get("height")}px`;
  });

  model.on("change:_layout", () => {
    const layoutJson = model.get("_layout");
    const selectParams = createSelectLayoutParams(layoutJson);
    viewer.selectLayout(selectParams);
  });

  el.appendChild(parent);
}

export default { render };
