import foxglove from "@foxglove/eslint-plugin";
import { defineConfig } from "eslint/config";
import globals from "globals";

export default defineConfig(
  {
    ignores: [
      ".cargo",
      "**/.venv",
      "**/dist",
      "cpp/build-*",
      "cpp/build",
      "python/foxglove-sdk/**/_build",
      "target",
    ],
  },
  ...foxglove.configs.base,
  {
    files: ["**/*.js"],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
  ...foxglove.configs.typescript.map((config) => ({
    ...config,
    files: ["**/*.ts"],
  })),
  {
    files: ["**/*.ts"],
    languageOptions: {
      parserOptions: {
        project: "tsconfig.json",
      },
    },
  },
);
