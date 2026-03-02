import { FoxgloveEnumSchema, FoxgloveMessageSchema } from "./types";

export function generateMarkdown(
  schemas: Iterable<FoxgloveMessageSchema>,
  enums: Iterable<FoxgloveEnumSchema>,
): string {
  const sortedSchemas = [...schemas].sort((a, b) => a.name.localeCompare(b.name));
  const sortedEnums = [...enums].sort((a, b) => a.name.localeCompare(b.name));

  return [
    `\
# Foxglove schemas

See [Foxglove Schemas documentation](https://docs.foxglove.dev/docs/visualization/message-schemas/introduction).

All schemas are generated from [schemas.ts](/internal/schemas.ts).`,

    [
      "## Optional fields",
      "",
      "Optional message fields may be omitted. When omitted, the consumer (e.g. a Foxglove panel) determines the behavior.",
      "",
      "If the IDL does not support optional fields (e.g. ROS) you must specify a value for the field.",
    ].join("\n"),

    "## Contents",

    [
      ...sortedEnums.map(({ name }) => `- [enum ${name}](#enum-${name.toLowerCase()})`),
      ...sortedSchemas.map(({ name }) => `- [${name}](#${name.toLowerCase()})`),
    ].join("\n"),
    "----",
    sortedEnums.map(
      (enumSchema) => `\
## enum ${enumSchema.name}

${enumSchema.description}

name | value | description
---- | ----- | -----------
${enumSchema.values
  .map((value) => `\`${value.name}\` | ${value.value} | ${value.description ?? ""}`)
  .join("\n")}

`,
    ),
    ...sortedSchemas.map(
      (schema) => `\
## ${schema.name}

${schema.description}

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
${schema.fields
  .map((field) => {
    const arraySuffix =
      typeof field.array === "number" ? `[${field.array}]` : field.array === true ? "[]" : "";
    let type: string;
    switch (field.type.type) {
      case "enum":
        type = `[enum ${field.type.enum.name}](#enum-${field.type.enum.name.toLowerCase()})`;
        break;
      case "nested":
        type = `[${field.type.schema.name}](#${field.type.schema.name.toLowerCase()})`;
        break;
      case "primitive":
        type = field.type.name;
        break;
    }
    const optionalSuffix = field.optional ? " (optional)" : "";
    return `\
<tr>
<td><code>${field.name}</code>${optionalSuffix}</td>
<td>

${type}${arraySuffix}

</td>
<td>

${field.description}

</td>
</tr>`;
  })
  .join("\n")}
</table>`,
    ),
  ]
    .flat()
    .join("\n\n");
}
