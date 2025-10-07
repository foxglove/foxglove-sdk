import type { RenderProps } from "@anywidget/types";
import { FoxgloveViewer, type DataSource, type OpaqueLayoutData } from "@foxglove/embed";

/* Specifies attributes defined with traitlets in ../src/foxglove_notebook/__init__.py */
interface WidgetModel {
  width: string;
  height: string;
  src: string;
  layout_data: OpaqueLayoutData;
  data: DataView<ArrayBuffer>;
}

function render({ model, el }: RenderProps<WidgetModel>): void {
  function getDataSource(): DataSource {
    // Read data from the model and convert it to a DataSource
    const data = model.get("data");
    const files = splitMergedData(data.buffer);

    return {
      type: "file",
      file: files,
    };
  }

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
    initialDataSource: getDataSource(),
    initialLayout: getLayoutData(),
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
    const layoutData = getLayoutData();

    viewer.setLayoutData(layoutData);
  });

  el.appendChild(parent);
}

function splitMergedData(buffer: ArrayBuffer): File[] {
  if (buffer.byteLength === 0) {
    return [];
  }

  const view = new DataView(buffer);
  let offset = 0;

  // Read the file count (4 bytes, big-endian)
  const fileCount = view.getUint32(offset);
  offset += 4;

  const files: File[] = [];
  const separator = new Uint8Array([0x00, 0xff, 0x00, 0xff]);

  for (let i = 0; i < fileCount; i++) {
    // Check separator (8 bytes: magic + file index)
    const expectedSeparator = new Uint8Array(buffer, offset, 4);
    const fileIndex = view.getUint32(offset + 4);

    if (!arraysEqual(expectedSeparator, separator) || fileIndex !== i) {
      throw new Error(`Invalid separator at position ${offset}`);
    }

    offset += 8;

    // Read file size (8 bytes, big-endian)
    const fileSize = Number(view.getBigUint64(offset));
    offset += 8;

    // Extract file data
    const fileData = buffer.slice(offset, offset + fileSize);
    if (fileData.byteLength !== fileSize) {
      throw new Error(`Expected ${fileSize} bytes but got ${fileData.byteLength}`);
    }

    // Create File object
    files.push(new File([fileData], `data-${i}.mcap`));
    offset += fileSize;
  }

  return files;
}

function arraysEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

export default { render };
