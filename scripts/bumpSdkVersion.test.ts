import { glob } from "glob";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { parse, type TomlTable } from "smol-toml";

import { testOnlyExports } from "./bumpSdkVersion";

async function readManifest(filename: string): Promise<TomlTable> {
  return parse(await readFile(filename, "utf8"));
}

describe("bumpSdkVersion", () => {
  let directory: string;

  beforeEach(async () => {
    directory = await mkdtemp(path.join(os.tmpdir(), "foxglove-sdk-version-"));
    jest.replaceProperty(process, "env", {
      ...process.env,
      GITHUB_OUTPUT: path.join(directory, "output"),
    });
    jest.spyOn(console, "log").mockImplementation(() => undefined);
  });

  afterEach(async () => {
    jest.restoreAllMocks();
    await rm(directory, { recursive: true, force: true });
  });

  // Two `cargo tree` runs can exceed Jest's default limit on a cold runner.
  it("bumps SDK packages and shared dependencies together while preserving the loader version", async () => {
    const files = new Map([
      [
        "Cargo.toml",
        `[workspace]
resolver = "2"
members = ["rust/foxglove", "rust/foxglove_derive", "rust/foxglove_data_loader"]
[workspace.package]
version = "0.29.2"
[workspace.dependencies]
foxglove = { version = "0.29.2", path = "rust/foxglove", default-features = false }
foxglove_derive = { version = "0.29.2", path = "rust/foxglove_derive" }
`,
      ],
      [
        "rust/foxglove/Cargo.toml",
        `[package]
name = "foxglove"
version.workspace = true
edition = "2021"
[dependencies]
foxglove_derive = { workspace = true, optional = true }
`,
      ],
      [
        "rust/foxglove_derive/Cargo.toml",
        `[package]
name = "foxglove_derive"
version.workspace = true
edition = "2021"
`,
      ],
      [
        "rust/foxglove_data_loader/Cargo.toml",
        `[package]
name = "foxglove_data_loader"
version = "0.2.0"
edition = "2021"
[dependencies]
foxglove.workspace = true
`,
      ],
      ["rust/foxglove/src/lib.rs", ""],
      ["rust/foxglove_derive/src/lib.rs", ""],
      ["rust/foxglove_data_loader/src/lib.rs", ""],
      ["python/foxglove-sdk/python/docs/version.py", 'SDK_VERSION = "0.29.2"\n'],
      ["cpp/foxglove/docs/version.py", 'SDK_VERSION = "0.29.2"\n'],
      ["cpp/CMakeLists.txt", "project(foxglove-sdk VERSION 0.29.2)\n"],
    ]);
    for (const [relative, content] of files) {
      const filename = path.join(directory, relative);
      await mkdir(path.dirname(filename), { recursive: true });
      await writeFile(filename, content);
    }

    for (const version of ["0.30.0", "0.31.0"]) {
      await testOnlyExports.main(`v${version}`, directory);
      const manifest = await readManifest(path.join(directory, "Cargo.toml"));
      expect(manifest.workspace).toMatchObject({
        package: { version },
        dependencies: {
          foxglove: { version, path: "rust/foxglove", "default-features": false },
          foxglove_derive: { version, path: "rust/foxglove_derive" },
        },
      });
      for (const relative of [
        "rust/foxglove/Cargo.toml",
        "rust/foxglove_derive/Cargo.toml",
        "rust/foxglove_data_loader/Cargo.toml",
      ]) {
        expect(await readFile(path.join(directory, relative), "utf8")).toBe(files.get(relative));
      }
      for (const relative of [
        "python/foxglove-sdk/python/docs/version.py",
        "cpp/foxglove/docs/version.py",
      ]) {
        expect(await readFile(path.join(directory, relative), "utf8")).toBe(
          `SDK_VERSION = "${version}"\n`,
        );
      }
      expect(await readFile(path.join(directory, "cpp/CMakeLists.txt"), "utf8")).toBe(
        `project(foxglove-sdk VERSION ${version})\n`,
      );
    }
    expect(await readFile(path.join(directory, "output"), "utf8")).toBe(
      "prev-version=0.29.2\nnew-version=0.30.0\nprev-version=0.30.0\nnew-version=0.31.0\n",
    );
  }, 60_000);
});

describe("SDK workspace dependencies", () => {
  const root = path.resolve(__dirname, "..");

  it("keeps shared SDK requirements aligned with the SDK package version", async () => {
    const manifest = await readManifest(path.join(root, "Cargo.toml"));
    const workspace = manifest.workspace as TomlTable;
    const sdkPackage = workspace.package as TomlTable;
    const dependencies = workspace.dependencies as TomlTable;
    for (const name of ["foxglove", "foxglove_derive"]) {
      const member = await readManifest(path.join(root, "rust", name, "Cargo.toml"));
      expect(member.package).toMatchObject({ version: { workspace: true } });
    }
    expect(dependencies.foxglove).toMatchObject({
      version: sdkPackage.version,
      path: "rust/foxglove",
      "default-features": false,
    });
    expect(dependencies.foxglove_derive).toMatchObject({
      version: sdkPackage.version,
      path: "rust/foxglove_derive",
    });
  });

  it("requires every workspace member to inherit its Foxglove dependencies", async () => {
    const manifest = await readManifest(path.join(root, "Cargo.toml"));
    const workspace = manifest.workspace as TomlTable;
    const members = workspace.members as string[];
    const excluded = workspace.exclude as string[];
    const files = await glob(
      members.map((member) => `${member}/Cargo.toml`),
      {
        cwd: root,
        ignore: excluded.map((member) => `${member}/Cargo.toml`),
      },
    );
    expect(files).toContain("rust/foxglove_data_loader/Cargo.toml");
    for (const filename of files) {
      const member = await readManifest(path.join(root, filename));
      const targets = (member.target ?? {}) as TomlTable;
      for (const section of [member, ...Object.values(targets)]) {
        for (const kind of ["dependencies", "dev-dependencies", "build-dependencies"]) {
          const dependencies = ((section as TomlTable)[kind] ?? {}) as TomlTable;
          for (const name of ["foxglove", "foxglove_derive"]) {
            const dependency = dependencies[name] as TomlTable | undefined;
            if (dependency == undefined) {
              continue;
            }
            expect(dependency).not.toHaveProperty("version");
            // A dev-dependency may be a bare path dependency instead. Cargo strips those when
            // packaging, which a crate published before its dev-dependency (foxglove_derive
            // before foxglove) relies on; a workspace-inherited one would carry the new version.
            if (kind === "dev-dependencies" && dependency.workspace == undefined) {
              expect({ filename, path: dependency.path }).toEqual({
                filename,
                path: `../${name}`,
              });
              continue;
            }
            expect({ filename, workspace: dependency.workspace }).toEqual({
              filename,
              workspace: true,
            });
            expect(dependency).not.toHaveProperty("path");
          }
        }
      }
    }
  });

  it("keeps foxglove_derive's dev-dependency on foxglove path-only so it can be published first", async () => {
    const manifest = await readManifest(path.join(root, "rust/foxglove_derive/Cargo.toml"));
    const dependency = (manifest["dev-dependencies"] as TomlTable).foxglove as TomlTable;
    expect(dependency).toMatchObject({ path: "../foxglove" });
    expect(dependency).not.toHaveProperty("workspace");
    expect(dependency).not.toHaveProperty("version");
  });
});
