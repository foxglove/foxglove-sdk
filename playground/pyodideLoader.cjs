const { Parser } = require("acorn");
const { importAssertions } = require("acorn-import-assertions");
const walk = require("acorn-walk");
const esbuild = require("esbuild");

const parser = Parser.extend(importAssertions);

class PyodideParser {
  constructor(source, options) {
    this.delta = 0;
    this.ast = parser.parse(source, {
      ecmaVersion: 2025,
      sourceType: options.isModule ? "module" : "script",
    });
    this.options = options;
    this.source = source;
  }

  parse() {
    walk.simple(this.ast, {
      ExpressionStatement: (node) => {
        this.walkExpressionStatement(node);
      },
    });
  }

  replace(statement, replacement) {
    const length = statement.end - statement.start;
    const start = this.source.slice(0, statement.start + this.delta);
    const end = this.source.slice(statement.end + this.delta);
    this.source = `${start}${replacement}${end}`;
    this.delta += replacement.length - length;
  }

  walkExpressionStatement(statement) {
    if (this.options.globalLoadPyodide) {
      return;
    }

    const assignment = statement.expression?.left?.object;
    if (assignment?.type !== "Identifier" || assignment?.name !== "globalThis") {
      return;
    }

    this.replace(statement, "({});");
  }
}

function addNamedExports(source, options) {
  if (options.isModule) {
    return source;
  }

  const lines = source.split("\n");
  const commonExports =
    "module.exports = {loadPyodide: loadPyodide.loadPyodide, version: loadPyodide.version};";
  for (let index = 0; index < lines.length; index++) {
    if (!lines[index].includes("sourceMappingURL")) {
      continue;
    }
    lines.splice(index, 0, commonExports);
    break;
  }
  return lines.join("\n");
}

module.exports = function pyodideLoader(source) {
  const options = this.getOptions();
  let banner = "module.exports =";
  let footer = "";
  let transformedSource = source;

  if (options.isModule) {
    transformedSource = esbuild.transformSync(transformedSource, {
      banner: "const module={exports:{}};",
      footer: "module.exports;",
      format: "cjs",
    }).code;
    banner = "const out =";
    footer = "export const loadPyodide = out.loadPyodide;\nexport const version = out.version;";
  }

  const parserInstance = new PyodideParser(transformedSource, options);
  parserInstance.parse();
  const finalSource = addNamedExports(parserInstance.source, options);
  return `${banner} eval(${JSON.stringify(finalSource)});\n${footer}`;
};
