import { spawnSync } from "node:child_process";

import { testOnlyExports } from "./checkDataLoaderRelease";

jest.mock("node:child_process", () => ({ spawnSync: jest.fn() }));

function release(version: string, sdkRequirement: string) {
  return { version, sdkRequirement };
}

function metadata(version = "0.2.1", requirement = "^0.30.0"): string {
  return JSON.stringify({
    packages: [
      {
        name: "foxglove_data_loader",
        version,
        dependencies: [{ name: "foxglove", req: requirement, kind: null }],
      },
    ],
  });
}

function indexEntry(
  version: string,
  requirement: string,
  { yanked = false }: { yanked?: boolean } = {},
): string {
  return JSON.stringify({
    name: "foxglove_data_loader",
    vers: version,
    deps: [{ name: "foxglove", req: requirement, kind: "normal" }],
    yanked,
  });
}

describe("checkCompatibility", () => {
  const published = [release("0.2.0", "^0.29.2")];

  it("rejects a loader patch that changes the SDK compatibility line", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.1", "^0.30.0"), published);
    }).toThrow("Increase the loader version to 0.3.0");
  });

  it.each([
    ["0.3.0", "^0.30.0"],
    ["0.2.1", "^0.29.3"],
    ["0.2.1", "0.29.2"],
  ])("allows loader %s with SDK %s", (version, requirement) => {
    expect(() => {
      testOnlyExports.checkCompatibility(release(version, requirement), published);
    }).not.toThrow();
  });

  it("checks older release lines even when a newer loader line exists", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.2", "^0.30.0"), [
        ...published,
        release("0.3.0", "^0.30.0"),
      ]);
    }).toThrow("Published loader 0.2.0 requires foxglove ^0.29.2");
  });

  it("checks every stable release in a line instead of only the newest", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.2", "^0.30.0"), [
        ...published,
        release("0.2.1", "^0.30.0"),
      ]);
    }).toThrow("Published loader 0.2.0");
  });

  it("keeps yanked releases in the compatibility check", () => {
    const releases = testOnlyExports.parseIndex(indexEntry("0.2.0", "^0.29.2", { yanked: true }));
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.1", "^0.30.0"), releases);
    }).toThrow("Increase the loader version to 0.3.0");
  });

  it("ignores prerelease baselines", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.0", "^0.30.0"), [
        release("0.2.0-rc.1", "^0.29.2"),
      ]);
    }).not.toThrow();
  });

  it("requires a major loader bump after 1.0", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("1.3.0", "^2.0.0"), [release("1.2.0", "^1.9.0")]);
    }).toThrow("Increase the loader version to 2.0.0");
    expect(() => {
      testOnlyExports.checkCompatibility(release("2.0.0", "^2.0.0"), [release("1.2.0", "^1.9.0")]);
    }).not.toThrow();
  });

  it("allows compatible SDK minor updates after 1.0", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.1", "^1.3.0"), [release("0.2.0", "^1.2.0")]);
    }).not.toThrow();
  });

  it("treats 0.0 SDK patch versions as incompatible", () => {
    expect(() => {
      testOnlyExports.checkCompatibility(release("0.2.1", "^0.0.2"), [release("0.2.0", "^0.0.1")]);
    }).toThrow("Increase the loader version to 0.3.0");
  });

  it.each(["*", ">=0.29.2", "~0.29.2", "0.29", "^0.30.0-rc.1"])(
    "rejects unsupported SDK requirements %s",
    (requirement) => {
      expect(() => {
        testOnlyExports.checkCompatibility(release("0.3.0", requirement), published);
      }).toThrow("Unsupported Foxglove requirement");
    },
  );
});

describe("registry and metadata parsing", () => {
  it("reads the inherited requirement from Cargo metadata", () => {
    expect(testOnlyExports.parseMetadata(metadata())).toEqual(release("0.2.1", "^0.30.0"));
  });

  it("reads newline separated registry entries", () => {
    expect(
      testOnlyExports.parseIndex(
        `${indexEntry("0.1.0", "^0.9.0")}\n${indexEntry("0.2.0", "^0.29.2")}\n`,
      ),
    ).toEqual([release("0.1.0", "^0.9.0"), release("0.2.0", "^0.29.2")]);
  });

  it.each(["", "invalid json", "{}", '{"name":"another_crate"}', indexEntry("invalid", "^0.29.2")])(
    "rejects invalid index data %s",
    (text) => {
      expect(() => testOnlyExports.parseIndex(text)).toThrow();
    },
  );

  it("rejects an index entry missing the SDK dependency", () => {
    expect(() =>
      testOnlyExports.parseIndex(
        JSON.stringify({ name: "foxglove_data_loader", vers: "0.2.0", deps: [] }),
      ),
    ).toThrow("Expected one Foxglove requirement");
  });

  it.each([
    "{}",
    '{"packages":[]}',
    '{"packages":[{"name":"foxglove_data_loader","version":"0.2.0","dependencies":[]}]}',
  ])("rejects incomplete Cargo metadata %s", (text) => {
    expect(() => testOnlyExports.parseMetadata(text)).toThrow();
  });
});

describe("main", () => {
  beforeEach(() => {
    jest.mocked(spawnSync).mockReturnValue({
      pid: 1,
      output: [],
      stdout: metadata(),
      stderr: "",
      status: 0,
      signal: null,
    });
    jest.spyOn(globalThis, "fetch").mockResolvedValue(new Response(indexEntry("0.2.0", "^0.29.2")));
    jest.spyOn(console, "log").mockImplementation(() => undefined);
  });

  afterEach(() => {
    jest.restoreAllMocks();
    jest.mocked(spawnSync).mockReset();
  });

  it("rejects the incompatible release using Cargo and registry data", async () => {
    await expect(testOnlyExports.main()).rejects.toThrow("Increase the loader version to 0.3.0");
    expect(jest.mocked(fetch).mock.calls[0]?.[0]).toBe(
      "https://index.crates.io/fo/xg/foxglove_data_loader",
    );
    expect(jest.mocked(fetch).mock.calls[0]?.[1]?.signal).toBeInstanceOf(AbortSignal);
  });

  it("allows the first release in a new loader line", async () => {
    jest.mocked(fetch).mockResolvedValue(new Response(indexEntry("0.1.0", "^0.9.0")));
    await expect(testOnlyExports.main()).resolves.toBeUndefined();
  });

  it("rejects HTTP errors", async () => {
    jest.mocked(fetch).mockResolvedValue(new Response("unavailable", { status: 503 }));
    await expect(testOnlyExports.main()).rejects.toThrow("HTTP 503");
  });

  it("rejects request timeouts", async () => {
    jest.mocked(fetch).mockRejectedValue(new Error("request timed out"));
    await expect(testOnlyExports.main()).rejects.toThrow("request timed out");
  });

  it("rejects Cargo failures before requesting registry data", async () => {
    jest.mocked(spawnSync).mockReturnValue({
      pid: 1,
      output: [],
      stdout: "",
      stderr: "invalid manifest",
      status: 101,
      signal: null,
    });
    await expect(testOnlyExports.main()).rejects.toThrow("invalid manifest");
    expect(fetch).not.toHaveBeenCalled();
  });
});
