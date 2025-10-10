import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer, type OpaqueLayoutData } from "@foxglove/embed";

// Specifies attributes defined with traitlets in ../python/foxglove/notebook/widget.py
interface WidgetModel {
  width: string;
  height: string;
  src: string;
  layout_data: OpaqueLayoutData;
}

type Message = {
  type: "update-data";
};

function render({ model, el }: RenderProps<WidgetModel>): void {
  function getLayoutData(): OpaqueLayoutData {
    // Read layout from the model and verify it is not empty
    const layoutData = model.get("layout_data");

    return JSON.stringify(layoutData) !== "{}" ? layoutData : undefined;
  }

  const parent = document.createElement("div");

  const viewer = new FoxgloveViewer({
    parent,
    src: model.get("src") !== "" ? model.get("src") : undefined,
    orgSlug: undefined,
    initialLayout: getLayoutData(),
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

  parent.style.width = model.get("width");
  parent.style.height = model.get("height");

  model.on("change:width", () => {
    parent.style.width = model.get("width");
  });

  model.on("change:height", () => {
    parent.style.height = model.get("height");
  });

  model.on("change:layout", () => {
    const layoutData = getLayoutData();

    viewer.setLayoutData(layoutData);
  });

  el.appendChild(parent);
}

export default { render };
