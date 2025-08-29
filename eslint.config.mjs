import foxglove from "@foxglove/eslint-plugin";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["**/dist", "python/foxglove-sdk/**/_build", ".cargo", "cpp/build"],
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
