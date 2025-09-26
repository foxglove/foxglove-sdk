import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer, type DataSource, type OpaqueLayoutData } from "@foxglove/embed";

/* Specifies attributes defined with traitlets in ../src/foxglove_notebook/__init__.py */
interface WidgetModel {
  width: string;
  height: string;
  src: string;
  orgSlug: string;
  layout: OpaqueLayoutData;
  _data: DataView<ArrayBuffer>;
}

function render({ model, el }: RenderProps<WidgetModel>): void {
  function getDataSource(): DataSource {
    // Read data from the model and convert it to a DataSource
    const data = model.get("_data");

    return {
      type: "file",
      file: new File([data.buffer], "data.mcap"),
    };
  }

  function getLayout(): OpaqueLayoutData {
    // Read layout from the model and verify it is not empty
    const layout = model.get("layout");

    return JSON.stringify(layout) !== "{}" ? layout : undefined;
  }

  const parent = document.createElement("div");

  const viewer = new FoxgloveViewer({
    parent,
    src: model.get("src") !== "" ? model.get("src") : undefined,
    orgSlug: model.get("orgSlug") !== "" ? model.get("orgSlug") : undefined,
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

    viewer.setDataSource(dataSource);
  });

  model.on("change:layout", () => {
    const layout = getLayout();

    viewer.setLayoutData(layout);
  });

  el.appendChild(parent);
}

export default { render };
