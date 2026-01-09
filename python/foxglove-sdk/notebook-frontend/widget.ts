import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer } from "@foxglove/embed";
import type { Layout, SelectLayoutParams } from "@foxglove/embed/src/types";

// Specifies attributes defined with traitlets in ../python/foxglove/notebook/widget.py
interface WidgetModel {
  width: number | "full";
  height: number;
  src?: string;
  _layout_params?: {
    storage_key: string;
    opaque_layout?: object;
    layout?: string;
    force: boolean;
  };
}

type Message = {
  type: "update-data";
};

function createSelectLayoutParams(
  layoutParams: WidgetModel["_layout_params"],
): SelectLayoutParams | undefined {
  if (!layoutParams) {
    return undefined;
  }

  return {
    storageKey: layoutParams.storage_key,
    force: layoutParams.force,
    ...(layoutParams.layout
      ? {
          layout: JSON.parse(layoutParams.layout) as Layout,
        }
      : {
          opaqueLayout: layoutParams.opaque_layout,
        }),
  };
}

function render({ model, el }: RenderProps<WidgetModel>): void {
  const parent = document.createElement("div");

  const initialLayoutParams = model.get("_layout_params");

  const viewer = new FoxgloveViewer({
    parent,
    embeddedViewer: "Python",
    src: model.get("src"),
    orgSlug: undefined,
    initialLayoutParams: createSelectLayoutParams(initialLayoutParams),
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

  model.on("change:_layout_params", () => {
    const layoutParams = model.get("_layout_params");
    const selectParams = createSelectLayoutParams(layoutParams);
    if (selectParams) {
      viewer.selectLayout(selectParams);
    }
  });

  el.appendChild(parent);
}

export default { render };
