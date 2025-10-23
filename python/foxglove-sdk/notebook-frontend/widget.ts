import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer, type SelectLayoutParams } from "@foxglove/embed";

// Specifies attributes defined with traitlets in ../python/foxglove/notebook/widget.py
interface WidgetModel {
  width: number | "full";
  height: number;
  src?: string;
  layout?: SelectLayoutParams;
}

type Message = {
  type: "update-data";
};

function render({ model, el }: RenderProps<WidgetModel>): void {
  const parent = document.createElement("div");

  const viewer = new FoxgloveViewer({
    parent,
    src: model.get("src"),
    orgSlug: undefined,
    initialLayoutParams: model.get("layout"),
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

  model.on("change:layout", () => {
    const layout = model.get("layout");

    if (layout) {
      viewer.selectLayout(layout);
    }
  });

  el.appendChild(parent);
}

export default { render };
