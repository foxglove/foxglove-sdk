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
      "python/foxglove-sdk/python/foxglove/notebook/static",
      "schemas/jsonschema",
      "target",
      "typescript/schemas/src/jsonschema",
      // Stub files for subpath imports
      "typescript/schemas/internal.d.ts",
      "typescript/schemas/internal.js",
      "typescript/schemas/jsonschema.d.ts",
      "typescript/schemas/jsonschema.js",
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
  ...foxglove.configs.react,
  ...foxglove.configs.typescript.map((config) => ({
    ...config,
    files: ["**/*.@(ts|tsx)"],
  })),
  {
    files: ["**/*.@(ts|tsx)"],
    languageOptions: {
      parserOptions: {
        project: true,
      },
    },
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          vars: "all",
          args: "after-used",
          varsIgnorePattern: "^_",
          argsIgnorePattern: "^_",
          caughtErrors: "none",
        },
      ],
    },
  },
);
