import zstd from "@foxglove/wasm-zstd";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { Playground } from "./Playground";
import { ThemeProvider } from "./ThemeProvider";

import "./index.css";

function LoadError(props: { value: string }) {
  return <>An error occurred: {props.value}</>;
}

zstd.isLoaded.then(
  () => {
    createRoot(document.getElementById("root")!).render(
      <StrictMode>
        <ThemeProvider>
          <Playground />
        </ThemeProvider>
      </StrictMode>,
    );
  },
  (err: unknown) => {
    createRoot(document.getElementById("root")!).render(
      <StrictMode>
        <LoadError value={String(err)} />
      </StrictMode>,
    );
  },
);
