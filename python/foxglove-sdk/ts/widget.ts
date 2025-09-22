import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer, type DataSource, type OpaqueLayoutData } from "@foxglove/embed";

/* Specifies attributes defined with traitlets in ../src/foxglove_notebook/__init__.py */
interface WidgetModel {
  width: string;
  height: string;
  src: string;
  orgSlug?: string;
  data?: DataView<ArrayBuffer>;
  layout?: OpaqueLayoutData;
}

function render({ model, el }: RenderProps<WidgetModel>): void {
  function getDataSource(): DataSource | undefined {
    // Read data from the model and convert it to a DataSource
    const data = model.get("data");

    return data != undefined
      ? {
          type: "file",
          file: new File([data.buffer], "data.mcap"),
        }
      : undefined;
  }

  function getLayout(): OpaqueLayoutData {
    // Read layout from the model and verify it is not empty
    const layout = model.get("layout");

    return JSON.stringify(layout) !== "{}" ? layout : undefined;
  }

  const parent = document.createElement("div");

  const viewer = new FoxgloveViewer({
    parent,
    src: model.get("src"),
    orgSlug: model.get("orgSlug") === "" ? undefined : model.get("orgSlug"),
    initialDataSource: getDataSource(),
    initialLayout: getLayout(),
  });

  parent.style.width = model.get("width");
  parent.style.height = model.get("height");

  model.on("change:width", () => {
    parent.style.width = model.get("width");
  });

  model.on("change:height", () => {
    parent.style.height = model.get("height");
  });

  model.on("change:data", () => {
    const dataSource = getDataSource();

    if (dataSource != undefined) {
      viewer.setDataSource(dataSource);
    }
  });

  model.on("change:layout", () => {
    const layout = getLayout();

    if (layout != undefined) {
      viewer.setLayoutData(layout);
    }
  });

  el.appendChild(parent);
}

export default { render };
